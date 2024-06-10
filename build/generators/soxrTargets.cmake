# Load the debug and release variables
file(GLOB DATA_FILES "${CMAKE_CURRENT_LIST_DIR}/soxr-*-data.cmake")

foreach(f ${DATA_FILES})
    include(${f})
endforeach()

# Create the targets for all the components
foreach(_COMPONENT ${soxr_COMPONENT_NAMES} )
    if(NOT TARGET ${_COMPONENT})
        add_library(${_COMPONENT} INTERFACE IMPORTED)
        message(${soxr_MESSAGE_MODE} "Conan: Component target declared '${_COMPONENT}'")
    endif()
endforeach()

if(NOT TARGET soxr::soxr)
    add_library(soxr::soxr INTERFACE IMPORTED)
    message(${soxr_MESSAGE_MODE} "Conan: Target declared 'soxr::soxr'")
endif()
# Load the debug and release library finders
file(GLOB CONFIG_FILES "${CMAKE_CURRENT_LIST_DIR}/soxr-Target-*.cmake")

foreach(f ${CONFIG_FILES})
    include(${f})
endforeach()