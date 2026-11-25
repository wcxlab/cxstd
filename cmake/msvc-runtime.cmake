# Match Rust's dynamic MSVC runtime in every configuration, including Debug.
# This property only affects compilers targeting the MSVC ABI.
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL" CACHE STRING "MSVC runtime library" FORCE)
