#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use super::super::ActiveAppInfo;
use cocoa::base::id;
use objc::{class, msg_send, sel, sel_impl};
use std::ffi::CStr;

pub fn get_active_app() -> Option<ActiveAppInfo> {
    unsafe {
        let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: id = msg_send![workspace, frontmostApplication];
        
        if app.is_null() {
            return None;
        }

        let localized_name: id = msg_send![app, localizedName];
        let bundle_id: id = msg_send![app, bundleIdentifier];
        let pid: i32 = msg_send![app, processIdentifier];
        
        let app_name = if !localized_name.is_null() {
            let utf8: *const std::os::raw::c_char = msg_send![localized_name, UTF8String];
            CStr::from_ptr(utf8).to_string_lossy().into_owned()
        } else {
            String::new()
        };
        
        let process_name = if !bundle_id.is_null() {
            let utf8: *const std::os::raw::c_char = msg_send![bundle_id, UTF8String];
            CStr::from_ptr(utf8).to_string_lossy().into_owned()
        } else {
            String::new()
        };

        Some(ActiveAppInfo {
            app_name,
            process_name,
            process_id: pid,
            window_title: None, // Can be extended with CGWindowListCopyWindowInfo later
        })
    }
}
