# Avoid multiple calls to find_package to append duplicated properties to the targets
include_guard()########### VARIABLES #######################################################################
#############################################################################################
set(soxr_FRAMEWORKS_FOUND_RELEASE "") # Will be filled later
conan_find_apple_frameworks(soxr_FRAMEWORKS_FOUND_RELEASE "${soxr_FRAMEWORKS_RELEASE}" "${soxr_FRAMEWORK_DIRS_RELEASE}")

set(soxr_LIBRARIES_TARGETS "") # Will be filled later


######## Create an interface target to contain all the dependencies (frameworks, system and conan deps)
if(NOT TARGET soxr_DEPS_TARGET)
    add_library(soxr_DEPS_TARGET INTERFACE IMPORTED)
endif()

set_property(TARGET soxr_DEPS_TARGET
             APPEND PROPERTY INTERFACE_LINK_LIBRARIES
             $<$<CONFIG:Release>:${soxr_FRAMEWORKS_FOUND_RELEASE}>
             $<$<CONFIG:Release>:${soxr_SYSTEM_LIBS_RELEASE}>
             $<$<CONFIG:Release>:soxr::core>)

####### Find the libraries declared in cpp_info.libs, create an IMPORTED target for each one and link the
####### soxr_DEPS_TARGET to all of them
conan_package_library_targets("${soxr_LIBS_RELEASE}"    # libraries
                              "${soxr_LIB_DIRS_RELEASE}" # package_libdir
                              "${soxr_BIN_DIRS_RELEASE}" # package_bindir
                              "${soxr_LIBRARY_TYPE_RELEASE}"
                              "${soxr_IS_HOST_WINDOWS_RELEASE}"
                              soxr_DEPS_TARGET
                              soxr_LIBRARIES_TARGETS  # out_libraries_targets
                              "_RELEASE"
                              "soxr"    # package_name
                              "${soxr_NO_SONAME_MODE_RELEASE}")  # soname

# FIXME: What is the result of this for multi-config? All configs adding themselves to path?
set(CMAKE_MODULE_PATH ${soxr_BUILD_DIRS_RELEASE} ${CMAKE_MODULE_PATH})

########## COMPONENTS TARGET PROPERTIES Release ########################################

    ########## COMPONENT soxr::lsr #############

        set(soxr_soxr_lsr_FRAMEWORKS_FOUND_RELEASE "")
        conan_find_apple_frameworks(soxr_soxr_lsr_FRAMEWORKS_FOUND_RELEASE "${soxr_soxr_lsr_FRAMEWORKS_RELEASE}" "${soxr_soxr_lsr_FRAMEWORK_DIRS_RELEASE}")

        set(soxr_soxr_lsr_LIBRARIES_TARGETS "")

        ######## Create an interface target to contain all the dependencies (frameworks, system and conan deps)
        if(NOT TARGET soxr_soxr_lsr_DEPS_TARGET)
            add_library(soxr_soxr_lsr_DEPS_TARGET INTERFACE IMPORTED)
        endif()

        set_property(TARGET soxr_soxr_lsr_DEPS_TARGET
                     APPEND PROPERTY INTERFACE_LINK_LIBRARIES
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_FRAMEWORKS_FOUND_RELEASE}>
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_SYSTEM_LIBS_RELEASE}>
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_DEPENDENCIES_RELEASE}>
                     )

        ####### Find the libraries declared in cpp_info.component["xxx"].libs,
        ####### create an IMPORTED target for each one and link the 'soxr_soxr_lsr_DEPS_TARGET' to all of them
        conan_package_library_targets("${soxr_soxr_lsr_LIBS_RELEASE}"
                              "${soxr_soxr_lsr_LIB_DIRS_RELEASE}"
                              "${soxr_soxr_lsr_BIN_DIRS_RELEASE}" # package_bindir
                              "${soxr_soxr_lsr_LIBRARY_TYPE_RELEASE}"
                              "${soxr_soxr_lsr_IS_HOST_WINDOWS_RELEASE}"
                              soxr_soxr_lsr_DEPS_TARGET
                              soxr_soxr_lsr_LIBRARIES_TARGETS
                              "_RELEASE"
                              "soxr_soxr_lsr"
                              "${soxr_soxr_lsr_NO_SONAME_MODE_RELEASE}")


        ########## TARGET PROPERTIES #####################################
        set_property(TARGET soxr::lsr
                     APPEND PROPERTY INTERFACE_LINK_LIBRARIES
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_OBJECTS_RELEASE}>
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_LIBRARIES_TARGETS}>
                     )

        if("${soxr_soxr_lsr_LIBS_RELEASE}" STREQUAL "")
            # If the component is not declaring any "cpp_info.components['foo'].libs" the system, frameworks etc are not
            # linked to the imported targets and we need to do it to the global target
            set_property(TARGET soxr::lsr
                         APPEND PROPERTY INTERFACE_LINK_LIBRARIES
                         soxr_soxr_lsr_DEPS_TARGET)
        endif()

        set_property(TARGET soxr::lsr APPEND PROPERTY INTERFACE_LINK_OPTIONS
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_LINKER_FLAGS_RELEASE}>)
        set_property(TARGET soxr::lsr APPEND PROPERTY INTERFACE_INCLUDE_DIRECTORIES
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_INCLUDE_DIRS_RELEASE}>)
        set_property(TARGET soxr::lsr APPEND PROPERTY INTERFACE_LINK_DIRECTORIES
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_LIB_DIRS_RELEASE}>)
        set_property(TARGET soxr::lsr APPEND PROPERTY INTERFACE_COMPILE_DEFINITIONS
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_COMPILE_DEFINITIONS_RELEASE}>)
        set_property(TARGET soxr::lsr APPEND PROPERTY INTERFACE_COMPILE_OPTIONS
                     $<$<CONFIG:Release>:${soxr_soxr_lsr_COMPILE_OPTIONS_RELEASE}>)

    ########## COMPONENT soxr::core #############

        set(soxr_soxr_core_FRAMEWORKS_FOUND_RELEASE "")
        conan_find_apple_frameworks(soxr_soxr_core_FRAMEWORKS_FOUND_RELEASE "${soxr_soxr_core_FRAMEWORKS_RELEASE}" "${soxr_soxr_core_FRAMEWORK_DIRS_RELEASE}")

        set(soxr_soxr_core_LIBRARIES_TARGETS "")

        ######## Create an interface target to contain all the dependencies (frameworks, system and conan deps)
        if(NOT TARGET soxr_soxr_core_DEPS_TARGET)
            add_library(soxr_soxr_core_DEPS_TARGET INTERFACE IMPORTED)
        endif()

        set_property(TARGET soxr_soxr_core_DEPS_TARGET
                     APPEND PROPERTY INTERFACE_LINK_LIBRARIES
                     $<$<CONFIG:Release>:${soxr_soxr_core_FRAMEWORKS_FOUND_RELEASE}>
                     $<$<CONFIG:Release>:${soxr_soxr_core_SYSTEM_LIBS_RELEASE}>
                     $<$<CONFIG:Release>:${soxr_soxr_core_DEPENDENCIES_RELEASE}>
                     )

        ####### Find the libraries declared in cpp_info.component["xxx"].libs,
        ####### create an IMPORTED target for each one and link the 'soxr_soxr_core_DEPS_TARGET' to all of them
        conan_package_library_targets("${soxr_soxr_core_LIBS_RELEASE}"
                              "${soxr_soxr_core_LIB_DIRS_RELEASE}"
                              "${soxr_soxr_core_BIN_DIRS_RELEASE}" # package_bindir
                              "${soxr_soxr_core_LIBRARY_TYPE_RELEASE}"
                              "${soxr_soxr_core_IS_HOST_WINDOWS_RELEASE}"
                              soxr_soxr_core_DEPS_TARGET
                              soxr_soxr_core_LIBRARIES_TARGETS
                              "_RELEASE"
                              "soxr_soxr_core"
                              "${soxr_soxr_core_NO_SONAME_MODE_RELEASE}")


        ########## TARGET PROPERTIES #####################################
        set_property(TARGET soxr::core
                     APPEND PROPERTY INTERFACE_LINK_LIBRARIES
                     $<$<CONFIG:Release>:${soxr_soxr_core_OBJECTS_RELEASE}>
                     $<$<CONFIG:Release>:${soxr_soxr_core_LIBRARIES_TARGETS}>
                     )

        if("${soxr_soxr_core_LIBS_RELEASE}" STREQUAL "")
            # If the component is not declaring any "cpp_info.components['foo'].libs" the system, frameworks etc are not
            # linked to the imported targets and we need to do it to the global target
            set_property(TARGET soxr::core
                         APPEND PROPERTY INTERFACE_LINK_LIBRARIES
                         soxr_soxr_core_DEPS_TARGET)
        endif()

        set_property(TARGET soxr::core APPEND PROPERTY INTERFACE_LINK_OPTIONS
                     $<$<CONFIG:Release>:${soxr_soxr_core_LINKER_FLAGS_RELEASE}>)
        set_property(TARGET soxr::core APPEND PROPERTY INTERFACE_INCLUDE_DIRECTORIES
                     $<$<CONFIG:Release>:${soxr_soxr_core_INCLUDE_DIRS_RELEASE}>)
        set_property(TARGET soxr::core APPEND PROPERTY INTERFACE_LINK_DIRECTORIES
                     $<$<CONFIG:Release>:${soxr_soxr_core_LIB_DIRS_RELEASE}>)
        set_property(TARGET soxr::core APPEND PROPERTY INTERFACE_COMPILE_DEFINITIONS
                     $<$<CONFIG:Release>:${soxr_soxr_core_COMPILE_DEFINITIONS_RELEASE}>)
        set_property(TARGET soxr::core APPEND PROPERTY INTERFACE_COMPILE_OPTIONS
                     $<$<CONFIG:Release>:${soxr_soxr_core_COMPILE_OPTIONS_RELEASE}>)

    ########## AGGREGATED GLOBAL TARGET WITH THE COMPONENTS #####################
    set_property(TARGET soxr::soxr APPEND PROPERTY INTERFACE_LINK_LIBRARIES soxr::lsr)
    set_property(TARGET soxr::soxr APPEND PROPERTY INTERFACE_LINK_LIBRARIES soxr::core)

########## For the modules (FindXXX)
set(soxr_LIBRARIES_RELEASE soxr::soxr)
