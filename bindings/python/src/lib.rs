use pyo3::{
    exceptions::{PyMemoryError, PyValueError},
    prelude::*,
    types::PyBytes,
};

fn convert_error(error: msrc::Error) -> PyErr {
    match error {
        msrc::Error::AllocationFailed => PyMemoryError::new_err(error.to_string()),
        _ => PyValueError::new_err(error.to_string()),
    }
}

/// Metadata for a checked MSRC envelope. Inspecting does not decode the replay.
#[pyclass(frozen, get_all, module = "msrc")]
struct ArchiveInfo {
    container_version: u16,
    algorithm_id: u16,
    algorithm_version: u16,
    original_size: u64,
    payload_size: u64,
    original_crc32: u32,
    payload_crc32: u32,
}

impl From<msrc::Header> for ArchiveInfo {
    fn from(header: msrc::Header) -> Self {
        Self {
            container_version: header.container_version,
            algorithm_id: header.algorithm_id,
            algorithm_version: header.algorithm_version,
            original_size: header.original_size,
            payload_size: header.payload_size,
            original_crc32: header.original_crc32,
            payload_crc32: header.payload_crc32,
        }
    }
}

#[pyfunction]
#[pyo3(signature = (data, *, level = 6))]
fn compress<'py>(py: Python<'py>, data: &[u8], level: u32) -> PyResult<Bound<'py, PyBytes>> {
    let output = py
        .detach(|| {
            msrc::compress_with_options(
                data,
                msrc::CompressionOptions {
                    level,
                    ..Default::default()
                },
            )
        })
        .map_err(convert_error)?;
    Ok(PyBytes::new(py, &output))
}

#[pyfunction]
#[pyo3(signature = (data, *, max_output_size = msrc::DEFAULT_MAX_OUTPUT_SIZE, max_memory_kib = msrc::DEFAULT_MAX_MEMORY_KIB))]
fn decompress<'py>(
    py: Python<'py>,
    data: &[u8],
    max_output_size: u64,
    max_memory_kib: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    let limits = msrc::DecompressionLimits {
        max_output_size,
        max_memory_kib,
    };
    let output = py
        .detach(|| msrc::decompress_with_limits(data, limits))
        .map_err(convert_error)?;
    Ok(PyBytes::new(py, &output))
}

#[pyfunction]
fn inspect(py: Python<'_>, data: &[u8]) -> PyResult<ArchiveInfo> {
    py.detach(|| msrc::inspect(data))
        .map(ArchiveInfo::from)
        .map_err(convert_error)
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(compress, module)?)?;
    module.add_function(wrap_pyfunction!(decompress, module)?)?;
    module.add_function(wrap_pyfunction!(inspect, module)?)?;
    module.add_class::<ArchiveInfo>()?;
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
