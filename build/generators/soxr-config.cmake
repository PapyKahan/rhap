########## MACROS ###########################################################################
#############################################################################################

# Requires CMake > 3.15
if(${CMAKE_VERSION} VERSION_LESS "3.15")
    message(FATAL_ERROR "The 'CMakeDeps' generator only works with CMake >= 3.15")
endif()

if(soxr_FIND_QUIETLY)
    set(soxr_MESSAGE_MODE VERBOSE)
else()
    set(soxr_MESSAGE_MODE STATUS)
endif()

include(${CMAKE_CURRENT_LIST_DIR}/cmakedeps_macros.cmake)
include(${CMAKE_CURRENT_LIST_DIR}/soxrTargets.cmake)
include(CMakeFindDependencyMacro)

check_build_type_defined()

foreach(_DEPENDENCY ${soxr_FIND_DEPENDENCY_NAMES} )
    # Check that we have not already called a find_package with the transitive dependency
    if(NOT ${_DEPENDENCY}_FOUND)
        find_dependency(${_DEPENDENCY} REQUIRED ${${_DEPENDENCY}_FIND_MODE})
    endif()
endforeach()

set(soxr_VERSION_STRING "0.1.3")
set(soxr_INCLUDE_DIRS ${soxr_INCLUDE_DIRS_RELEASE} )
set(soxr_INCLUDE_DIR ${soxr_INCLUDE_DIRS_RELEASE} )
set(soxr_LIBRARIES ${soxr_LIBRARIES_RELEASE} )
set(soxr_DEFINITIONS ${soxr_DEFINITIONS_RELEASE} )


# Only the last installed configuration BUILD_MODULES are included to avoid the collision
foreach(_BUILD_MODULE ${soxr_BUILD_MODULES_PATHS_RELEASE} )
    message(${soxr_MESSAGE_MODE} "Conan: Including build module from '${_BUILD_MODULE}'")
    include(${_BUILD_MODULE})
endforeach()


