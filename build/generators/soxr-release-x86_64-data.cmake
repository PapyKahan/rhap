########### AGGREGATED COMPONENTS AND DEPENDENCIES FOR THE MULTI CONFIG #####################
#############################################################################################

list(APPEND soxr_COMPONENT_NAMES soxr::core soxr::lsr)
list(REMOVE_DUPLICATES soxr_COMPONENT_NAMES)
if(DEFINED soxr_FIND_DEPENDENCY_NAMES)
  list(APPEND soxr_FIND_DEPENDENCY_NAMES )
  list(REMOVE_DUPLICATES soxr_FIND_DEPENDENCY_NAMES)
else()
  set(soxr_FIND_DEPENDENCY_NAMES )
endif()

########### VARIABLES #######################################################################
#############################################################################################
set(soxr_PACKAGE_FOLDER_RELEASE "C:/Users/U118120/.conan2/p/soxr78e5a3a4fcfda/p")
set(soxr_BUILD_MODULES_PATHS_RELEASE )


set(soxr_INCLUDE_DIRS_RELEASE "${soxr_PACKAGE_FOLDER_RELEASE}/include")
set(soxr_RES_DIRS_RELEASE )
set(soxr_DEFINITIONS_RELEASE )
set(soxr_SHARED_LINK_FLAGS_RELEASE )
set(soxr_EXE_LINK_FLAGS_RELEASE )
set(soxr_OBJECTS_RELEASE )
set(soxr_COMPILE_DEFINITIONS_RELEASE )
set(soxr_COMPILE_OPTIONS_C_RELEASE )
set(soxr_COMPILE_OPTIONS_CXX_RELEASE )
set(soxr_LIB_DIRS_RELEASE "${soxr_PACKAGE_FOLDER_RELEASE}/lib")
set(soxr_BIN_DIRS_RELEASE )
set(soxr_LIBRARY_TYPE_RELEASE STATIC)
set(soxr_IS_HOST_WINDOWS_RELEASE 1)
set(soxr_LIBS_RELEASE soxr-lsr soxr)
set(soxr_SYSTEM_LIBS_RELEASE )
set(soxr_FRAMEWORK_DIRS_RELEASE )
set(soxr_FRAMEWORKS_RELEASE )
set(soxr_BUILD_DIRS_RELEASE )
set(soxr_NO_SONAME_MODE_RELEASE FALSE)


# COMPOUND VARIABLES
set(soxr_COMPILE_OPTIONS_RELEASE
    "$<$<COMPILE_LANGUAGE:CXX>:${soxr_COMPILE_OPTIONS_CXX_RELEASE}>"
    "$<$<COMPILE_LANGUAGE:C>:${soxr_COMPILE_OPTIONS_C_RELEASE}>")
set(soxr_LINKER_FLAGS_RELEASE
    "$<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,SHARED_LIBRARY>:${soxr_SHARED_LINK_FLAGS_RELEASE}>"
    "$<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,MODULE_LIBRARY>:${soxr_SHARED_LINK_FLAGS_RELEASE}>"
    "$<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,EXECUTABLE>:${soxr_EXE_LINK_FLAGS_RELEASE}>")


set(soxr_COMPONENTS_RELEASE soxr::core soxr::lsr)
########### COMPONENT soxr::lsr VARIABLES ############################################

set(soxr_soxr_lsr_INCLUDE_DIRS_RELEASE "${soxr_PACKAGE_FOLDER_RELEASE}/include")
set(soxr_soxr_lsr_LIB_DIRS_RELEASE "${soxr_PACKAGE_FOLDER_RELEASE}/lib")
set(soxr_soxr_lsr_BIN_DIRS_RELEASE )
set(soxr_soxr_lsr_LIBRARY_TYPE_RELEASE STATIC)
set(soxr_soxr_lsr_IS_HOST_WINDOWS_RELEASE 1)
set(soxr_soxr_lsr_RES_DIRS_RELEASE )
set(soxr_soxr_lsr_DEFINITIONS_RELEASE )
set(soxr_soxr_lsr_OBJECTS_RELEASE )
set(soxr_soxr_lsr_COMPILE_DEFINITIONS_RELEASE )
set(soxr_soxr_lsr_COMPILE_OPTIONS_C_RELEASE "")
set(soxr_soxr_lsr_COMPILE_OPTIONS_CXX_RELEASE "")
set(soxr_soxr_lsr_LIBS_RELEASE soxr-lsr)
set(soxr_soxr_lsr_SYSTEM_LIBS_RELEASE )
set(soxr_soxr_lsr_FRAMEWORK_DIRS_RELEASE )
set(soxr_soxr_lsr_FRAMEWORKS_RELEASE )
set(soxr_soxr_lsr_DEPENDENCIES_RELEASE soxr::core)
set(soxr_soxr_lsr_SHARED_LINK_FLAGS_RELEASE )
set(soxr_soxr_lsr_EXE_LINK_FLAGS_RELEASE )
set(soxr_soxr_lsr_NO_SONAME_MODE_RELEASE FALSE)

# COMPOUND VARIABLES
set(soxr_soxr_lsr_LINKER_FLAGS_RELEASE
        $<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,SHARED_LIBRARY>:${soxr_soxr_lsr_SHARED_LINK_FLAGS_RELEASE}>
        $<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,MODULE_LIBRARY>:${soxr_soxr_lsr_SHARED_LINK_FLAGS_RELEASE}>
        $<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,EXECUTABLE>:${soxr_soxr_lsr_EXE_LINK_FLAGS_RELEASE}>
)
set(soxr_soxr_lsr_COMPILE_OPTIONS_RELEASE
    "$<$<COMPILE_LANGUAGE:CXX>:${soxr_soxr_lsr_COMPILE_OPTIONS_CXX_RELEASE}>"
    "$<$<COMPILE_LANGUAGE:C>:${soxr_soxr_lsr_COMPILE_OPTIONS_C_RELEASE}>")
########### COMPONENT soxr::core VARIABLES ############################################

set(soxr_soxr_core_INCLUDE_DIRS_RELEASE "${soxr_PACKAGE_FOLDER_RELEASE}/include")
set(soxr_soxr_core_LIB_DIRS_RELEASE "${soxr_PACKAGE_FOLDER_RELEASE}/lib")
set(soxr_soxr_core_BIN_DIRS_RELEASE )
set(soxr_soxr_core_LIBRARY_TYPE_RELEASE STATIC)
set(soxr_soxr_core_IS_HOST_WINDOWS_RELEASE 1)
set(soxr_soxr_core_RES_DIRS_RELEASE )
set(soxr_soxr_core_DEFINITIONS_RELEASE )
set(soxr_soxr_core_OBJECTS_RELEASE )
set(soxr_soxr_core_COMPILE_DEFINITIONS_RELEASE )
set(soxr_soxr_core_COMPILE_OPTIONS_C_RELEASE "")
set(soxr_soxr_core_COMPILE_OPTIONS_CXX_RELEASE "")
set(soxr_soxr_core_LIBS_RELEASE soxr)
set(soxr_soxr_core_SYSTEM_LIBS_RELEASE )
set(soxr_soxr_core_FRAMEWORK_DIRS_RELEASE )
set(soxr_soxr_core_FRAMEWORKS_RELEASE )
set(soxr_soxr_core_DEPENDENCIES_RELEASE )
set(soxr_soxr_core_SHARED_LINK_FLAGS_RELEASE )
set(soxr_soxr_core_EXE_LINK_FLAGS_RELEASE )
set(soxr_soxr_core_NO_SONAME_MODE_RELEASE FALSE)

# COMPOUND VARIABLES
set(soxr_soxr_core_LINKER_FLAGS_RELEASE
        $<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,SHARED_LIBRARY>:${soxr_soxr_core_SHARED_LINK_FLAGS_RELEASE}>
        $<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,MODULE_LIBRARY>:${soxr_soxr_core_SHARED_LINK_FLAGS_RELEASE}>
        $<$<STREQUAL:$<TARGET_PROPERTY:TYPE>,EXECUTABLE>:${soxr_soxr_core_EXE_LINK_FLAGS_RELEASE}>
)
set(soxr_soxr_core_COMPILE_OPTIONS_RELEASE
    "$<$<COMPILE_LANGUAGE:CXX>:${soxr_soxr_core_COMPILE_OPTIONS_CXX_RELEASE}>"
    "$<$<COMPILE_LANGUAGE:C>:${soxr_soxr_core_COMPILE_OPTIONS_C_RELEASE}>")