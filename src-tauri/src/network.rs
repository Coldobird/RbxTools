//! OS-owned, non-persistent WFP filters. Closing the engine OR process termination
//! removes every filter belonging to the dynamic session, without a watchdog.
use std::{path::Path, ptr};
use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::HANDLE, NetworkManagement::WindowsFilteringPlatform::*,
        System::Rpc::RPC_C_AUTHN_WINNT,
    },
};

pub struct NetworkBlock {
    engine: usize,
}

fn check(code: u32, operation: &str) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{operation} failed (Windows 0x{code:08X}). Make sure RBX Tools is running as administrator and the Base Filtering Engine service is running."))
    }
}

impl NetworkBlock {
    pub fn start(path: &Path) -> Result<Self, String> {
        use std::os::windows::ffi::OsStrExt;
        let path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            let session = FWPM_SESSION0 {
                flags: FWPM_SESSION_FLAG_DYNAMIC,
                ..std::mem::zeroed()
            };
            let mut handle: HANDLE = ptr::null_mut();
            check(
                FwpmEngineOpen0(
                    ptr::null(),
                    RPC_C_AUTHN_WINNT,
                    ptr::null(),
                    &session,
                    &mut handle,
                ),
                "Opening temporary network session",
            )?;
            let block = Self {
                engine: handle as usize,
            };
            let mut app_id: *mut FWP_BYTE_BLOB = ptr::null_mut();
            check(
                FwpmGetAppIdFromFileName0(path_wide.as_ptr(), &mut app_id),
                "Identifying Steam",
            )?;
            struct Blob(*mut FWP_BYTE_BLOB);
            impl Drop for Blob {
                fn drop(&mut self) {
                    unsafe {
                        FwpmFreeMemory0(&mut self.0 as *mut _ as *mut *mut std::ffi::c_void);
                    }
                }
            }
            let _blob = Blob(app_id);
            check(
                FwpmTransactionBegin0(handle, 0),
                "Starting network transaction",
            )?;
            let mut label: Vec<u16> = "RBX Tools — temporary Steam block\0"
                .encode_utf16()
                .collect();
            let sublayer_key = GUID::from_u128(0x68b17ba8_f37e_4d7e_937b_c2727885438b);
            let sublayer = FWPM_SUBLAYER0 {
                subLayerKey: sublayer_key,
                displayData: FWPM_DISPLAY_DATA0 {
                    name: label.as_mut_ptr(),
                    description: ptr::null_mut(),
                },
                weight: 0x100,
                ..std::mem::zeroed()
            };
            check(
                FwpmSubLayerAdd0(handle, &sublayer, ptr::null_mut()),
                "Creating temporary filter group",
            )?;
            let application = FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_ALE_APP_ID,
                matchType: FWP_MATCH_EQUAL,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_BYTE_BLOB_TYPE,
                    Anonymous: FWP_CONDITION_VALUE0_0 { byteBlob: app_id },
                },
            };
            // Keep Steam's localhost communication with its UI and helpers.
            // Only traffic leaving/entering this computer should be paused.
            let external = FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_FLAGS,
                matchType: FWP_MATCH_FLAGS_NONE_SET,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_UINT32,
                    Anonymous: FWP_CONDITION_VALUE0_0 {
                        uint32: FWP_CONDITION_FLAG_IS_LOOPBACK,
                    },
                },
            };
            let mut conditions = [application, external];
            for layer in [
                FWPM_LAYER_ALE_AUTH_CONNECT_V4,
                FWPM_LAYER_ALE_AUTH_CONNECT_V6,
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
            ] {
                let filter = FWPM_FILTER0 {
                    displayData: FWPM_DISPLAY_DATA0 {
                        name: label.as_mut_ptr(),
                        description: ptr::null_mut(),
                    },
                    layerKey: layer,
                    subLayerKey: sublayer_key,
                    weight: FWP_VALUE0 {
                        r#type: FWP_UINT8,
                        Anonymous: FWP_VALUE0_0 { uint8: 15 },
                    },
                    numFilterConditions: conditions.len() as u32,
                    filterCondition: conditions.as_mut_ptr(),
                    action: FWPM_ACTION0 {
                        r#type: FWP_ACTION_BLOCK,
                        ..std::mem::zeroed()
                    },
                    ..std::mem::zeroed()
                };
                check(
                    FwpmFilterAdd0(handle, &filter, ptr::null_mut(), ptr::null_mut()),
                    "Adding application filter",
                )?;
            }
            check(
                FwpmTransactionCommit0(handle),
                "Applying temporary network block",
            )?;
            Ok(block)
        }
    }

    pub fn close(&mut self) -> Result<(), String> {
        if self.engine != 0 {
            unsafe {
                check(FwpmEngineClose0(self.engine as HANDLE), "Restoring network")?;
            }
            self.engine = 0;
        }
        Ok(())
    }
}

impl Drop for NetworkBlock {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
