//! Late-bound COM: an object reached by the names of its methods and
//! properties through IDispatch, as a script reaches it. VirtualBox gives
//! its interfaces new IDs whenever they change and keeps their names, so
//! calling by name keeps vma apart from those IDs; which versions it works
//! with is `core::version`'s. Nothing here knows VirtualBox: it turns names
//! and values into calls, and a failed call into the text its object gave.

use std::mem::ManuallyDrop;
use std::ptr::null_mut;
use windows::Win32::Foundation::{DISP_E_UNKNOWNNAME, VARIANT_FALSE, VARIANT_TRUE};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CLSIDFromProgID, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    DISPATCH_FLAGS, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS,
    EXCEPINFO, IDispatch, SAFEARRAY,
};
use windows::Win32::System::Ole::{
    DISPID_PROPERTYPUT, SafeArrayAccessData, SafeArrayCreateVector, SafeArrayGetLBound,
    SafeArrayGetUBound, SafeArrayPutElement, SafeArrayUnaccessData,
};
use windows::Win32::System::Variant::{
    VAR_CHANGE_FLAGS, VARENUM, VARIANT, VT_ARRAY, VT_BOOL, VT_BSTR, VT_BYREF, VT_DISPATCH,
    VT_EMPTY, VT_I4, VT_I8, VT_NULL, VT_UI1, VT_UI4, VT_UNKNOWN, VariantChangeType, VariantClear,
    VariantCopy,
};
use windows::core::{BSTR, GUID, IUnknown, Interface, PCWSTR};

/// COM for this thread, once, before any object is made. Free-threaded:
/// VirtualBox's objects live in its own processes, and a call goes straight
/// there instead of through a message loop vma doesn't run.
pub fn init() -> Result<(), String> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|e| format!("COM didn't start: {}", e.message()))
}

pub struct Object(IDispatch);

/// A value a call takes. An out value is written by the call.
pub enum Arg<'a> {
    U32(u32),
    I32(i32),
    I64(i64),
    Bool(bool),
    Str(&'a str),
    Object(&'a Object),
    /// No object where one may be given, as an empty optical drive's medium.
    NoObject,
    /// No value, for an optional argument left out, as the task
    /// scheduler's user and password.
    Empty,
    /// An array of strings, as a machine's groups.
    Strs(&'a [&'a str]),
    /// An array of an enum's numbers, as a medium's variants.
    I32s(&'a [i32]),
    /// An array of objects, as the media a machine's removal deletes.
    Objects(&'a [&'a Object]),
    /// A value an earlier call gave, passed on as it is, as the media
    /// `Unregister` gives and `DeleteConfig` takes.
    Value(&'a Value),
    OutU32(&'a mut u32),
    OutI32(&'a mut i32),
}

/// What a call gave back, freed when dropped.
pub struct Value(VARIANT);

impl Object {
    /// A new object of the class a program ID names, such as
    /// `VirtualBox.Session`.
    pub fn create(program: &str) -> Result<Object, String> {
        let wide: Vec<u16> = program.encode_utf16().chain([0]).collect();
        unsafe {
            let class = CLSIDFromProgID(PCWSTR(wide.as_ptr()))
                .map_err(|_| format!("{program} is not registered: is VirtualBox installed?"))?;
            CoCreateInstance(&class, None, CLSCTX_ALL)
                .map(Object)
                .map_err(|e| format!("{program} didn't start: {}", said(&e)))
        }
    }

    pub fn get(&self, name: &str) -> Result<Value, String> {
        self.invoke(name, DISPATCH_PROPERTYGET, &mut [])
    }

    pub fn call(&self, name: &str, args: &mut [Arg]) -> Result<Value, String> {
        self.invoke(name, DISPATCH_METHOD, args)
    }

    pub fn set(&self, name: &str, mut value: Arg) -> Result<(), String> {
        self.invoke(name, DISPATCH_PROPERTYPUT, std::slice::from_mut(&mut value))
            .map(drop)
    }

    fn invoke(&self, name: &str, flags: DISPATCH_FLAGS, args: &mut [Arg]) -> Result<Value, String> {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let names = [PCWSTR(wide.as_ptr())];
        let mut id = 0;
        unsafe {
            self.0
                .GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 1, 0, &mut id)
        }
        .map_err(|e| {
            if e.code() == DISP_E_UNKNOWNNAME {
                format!("this VirtualBox has no {name}")
            } else {
                format!("{name} failed: {}", said(&e))
            }
        })?;
        // IDispatch takes the arguments last first.
        let mut variants = args
            .iter_mut()
            .rev()
            .map(variant)
            .collect::<Result<Vec<VARIANT>, String>>()?;
        // A property's new value goes as the one named argument COM keeps
        // for it.
        let mut put = DISPID_PROPERTYPUT;
        let params = DISPPARAMS {
            rgvarg: variants.as_mut_ptr(),
            rgdispidNamedArgs: if flags == DISPATCH_PROPERTYPUT {
                &mut put
            } else {
                null_mut()
            },
            cArgs: variants.len() as u32,
            cNamedArgs: (flags == DISPATCH_PROPERTYPUT) as u32,
        };
        let mut result = VARIANT::default();
        let mut exception = EXCEPINFO::default();
        let called = unsafe {
            self.0.Invoke(
                id,
                &GUID::zeroed(),
                0,
                flags,
                &params,
                Some(&mut result),
                Some(&mut exception),
                None,
            )
        };
        for v in &mut variants {
            unsafe {
                let _ = VariantClear(v);
            }
        }
        match called {
            Ok(()) => Ok(Value(result)),
            Err(e) => Err(failure(name, &e, exception)),
        }
    }
}

fn variant(arg: &mut Arg) -> Result<VARIANT, String> {
    let mut v = VARIANT::default();
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        match arg {
            Arg::U32(n) => {
                inner.vt = VT_UI4;
                inner.Anonymous.ulVal = *n;
            }
            Arg::I32(n) => {
                inner.vt = VT_I4;
                inner.Anonymous.lVal = *n;
            }
            Arg::I64(n) => {
                inner.vt = VT_I8;
                inner.Anonymous.llVal = *n;
            }
            Arg::Bool(b) => {
                inner.vt = VT_BOOL;
                inner.Anonymous.boolVal = if *b { VARIANT_TRUE } else { VARIANT_FALSE };
            }
            Arg::Strs(items) => {
                let array = SafeArrayCreateVector(VT_BSTR, 0, items.len() as u32);
                for (i, item) in items.iter().enumerate() {
                    // The array takes its own copy of the string.
                    let text = BSTR::from(*item);
                    SafeArrayPutElement(array, &(i as i32), text.as_ptr() as _)
                        .map_err(|e| said(&e))?;
                }
                inner.vt = VARENUM(VT_ARRAY.0 | VT_BSTR.0);
                inner.Anonymous.parray = array;
            }
            Arg::I32s(items) => {
                let array = SafeArrayCreateVector(VT_I4, 0, items.len() as u32);
                for (i, item) in items.iter().enumerate() {
                    SafeArrayPutElement(array, &(i as i32), item as *const i32 as _)
                        .map_err(|e| said(&e))?;
                }
                inner.vt = VARENUM(VT_ARRAY.0 | VT_I4.0);
                inner.Anonymous.parray = array;
            }
            Arg::Objects(items) => {
                let array = SafeArrayCreateVector(VT_DISPATCH, 0, items.len() as u32);
                for (i, item) in items.iter().enumerate() {
                    // The array takes its own reference.
                    SafeArrayPutElement(array, &(i as i32), item.0.as_raw())
                        .map_err(|e| said(&e))?;
                }
                inner.vt = VARENUM(VT_ARRAY.0 | VT_DISPATCH.0);
                inner.Anonymous.parray = array;
            }
            Arg::Value(value) => VariantCopy(&mut v, &value.0).map_err(|e| said(&e))?,
            Arg::Str(s) => {
                inner.vt = VT_BSTR;
                inner.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(*s));
            }
            Arg::Object(o) => {
                inner.vt = VT_DISPATCH;
                inner.Anonymous.pdispVal = ManuallyDrop::new(Some(o.0.clone()));
            }
            Arg::NoObject => {
                inner.vt = VT_DISPATCH;
                inner.Anonymous.pdispVal = ManuallyDrop::new(None);
            }
            Arg::Empty => inner.vt = VT_EMPTY,
            Arg::OutU32(out) => {
                inner.vt = VARENUM(VT_BYREF.0 | VT_UI4.0);
                inner.Anonymous.pulVal = &mut **out;
            }
            Arg::OutI32(out) => {
                inner.vt = VARENUM(VT_BYREF.0 | VT_I4.0);
                inner.Anonymous.plVal = &mut **out;
            }
        }
    }
    Ok(v)
}

/// The text the object gave for a failed call, or the call's name and its
/// code where it gave none.
fn failure(name: &str, error: &windows::core::Error, mut exception: EXCEPINFO) -> String {
    unsafe {
        if let Some(fill) = exception.pfnDeferredFillIn {
            let _ = fill(&mut exception);
        }
    }
    let text = exception.bstrDescription.to_string();
    unsafe {
        ManuallyDrop::drop(&mut exception.bstrSource);
        ManuallyDrop::drop(&mut exception.bstrDescription);
        ManuallyDrop::drop(&mut exception.bstrHelpFile);
    }
    let text = text.trim();
    if text.is_empty() {
        format!("{name} failed: {}", said(error))
    } else {
        text.to_string()
    }
}

/// Windows' words for an error, or its code where Windows has none, as for
/// VirtualBox's own codes.
fn said(error: &windows::core::Error) -> String {
    let message = error.message();
    let message = message.trim();
    if message.is_empty() {
        format!("{:#010x}", error.code().0)
    } else {
        message.to_string()
    }
}

impl Value {
    fn vt(&self) -> VARENUM {
        unsafe { self.0.Anonymous.Anonymous.vt }
    }

    /// The object it holds; `None` where it holds none, as a session's
    /// console holds none while its machine is off.
    pub fn object(&self) -> Result<Option<Object>, String> {
        unsafe {
            let inner = &self.0.Anonymous.Anonymous.Anonymous;
            match self.vt() {
                VT_DISPATCH => Ok((*inner.pdispVal).clone().map(Object)),
                VT_UNKNOWN => match &*inner.punkVal {
                    Some(unknown) => as_object(unknown).map(Some),
                    None => Ok(None),
                },
                VT_EMPTY | VT_NULL => Ok(None),
                other => Err(unexpected("an object", other)),
            }
        }
    }

    pub fn u32(&self) -> Result<u32, String> {
        let changed = self.changed(VT_UI4)?;
        Ok(unsafe { changed.0.Anonymous.Anonymous.Anonymous.ulVal })
    }

    pub fn i32(&self) -> Result<i32, String> {
        let changed = self.changed(VT_I4)?;
        Ok(unsafe { changed.0.Anonymous.Anonymous.Anonymous.lVal })
    }

    pub fn i64(&self) -> Result<i64, String> {
        let changed = self.changed(VT_I8)?;
        Ok(unsafe { changed.0.Anonymous.Anonymous.Anonymous.llVal })
    }

    pub fn bool(&self) -> Result<bool, String> {
        let changed = self.changed(VT_BOOL)?;
        Ok(unsafe { changed.0.Anonymous.Anonymous.Anonymous.boolVal.as_bool() })
    }

    pub fn string(&self) -> Result<String, String> {
        let changed = self.changed(VT_BSTR)?;
        Ok(unsafe { changed.0.Anonymous.Anonymous.Anonymous.bstrVal.to_string() })
    }

    /// A copy changed to `to`, by COM's own rules, as a number VirtualBox
    /// gives signed where vma wants it unsigned.
    fn changed(&self, to: VARENUM) -> Result<Value, String> {
        let mut out = Value(VARIANT::default());
        unsafe { VariantChangeType(&mut out.0, &self.0, VAR_CHANGE_FLAGS(0), to) }
            .map_err(|_| unexpected(&format!("a value of type {}", to.0), self.vt()))?;
        Ok(out)
    }

    /// The bytes of an array of octets.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        if self.vt() != VARENUM(VT_ARRAY.0 | VT_UI1.0) {
            return Err(unexpected("bytes", self.vt()));
        }
        unsafe {
            let array = self.0.Anonymous.Anonymous.Anonymous.parray;
            with_elements::<u8, _>(array, |items| items.to_vec())
        }
    }

    /// The numbers of an array of 32-bit ones, signed or not, as COM gives
    /// an array of an enum's or of indexes.
    pub fn u32s(&self) -> Result<Vec<u32>, String> {
        let vt = self.vt();
        if vt != VARENUM(VT_ARRAY.0 | VT_UI4.0) && vt != VARENUM(VT_ARRAY.0 | VT_I4.0) {
            return Err(unexpected("numbers", vt));
        }
        unsafe {
            let array = self.0.Anonymous.Anonymous.Anonymous.parray;
            with_elements::<u32, _>(array, |items| items.to_vec())
        }
    }

    /// The strings of an array of them.
    pub fn strings(&self) -> Result<Vec<String>, String> {
        if self.vt() != VARENUM(VT_ARRAY.0 | VT_BSTR.0) {
            return Err(unexpected("strings", self.vt()));
        }
        unsafe {
            let array = self.0.Anonymous.Anonymous.Anonymous.parray;
            // The array owns its strings: each is read, never freed here.
            with_elements::<ManuallyDrop<BSTR>, _>(array, |items| {
                items.iter().map(|s| s.to_string()).collect()
            })
        }
    }

    /// The objects of an array of them.
    pub fn objects(&self) -> Result<Vec<Object>, String> {
        let vt = self.vt();
        if vt != VARENUM(VT_ARRAY.0 | VT_DISPATCH.0) && vt != VARENUM(VT_ARRAY.0 | VT_UNKNOWN.0) {
            return Err(unexpected("objects", vt));
        }
        unsafe {
            let array = self.0.Anonymous.Anonymous.Anonymous.parray;
            with_elements::<*mut std::ffi::c_void, _>(array, |items| {
                items
                    .iter()
                    .filter_map(|raw| IUnknown::from_raw_borrowed(raw))
                    .map(as_object)
                    .collect::<Result<Vec<_>, _>>()
            })?
        }
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        unsafe {
            let _ = VariantClear(&mut self.0);
        }
    }
}

fn as_object(unknown: &IUnknown) -> Result<Object, String> {
    unknown.cast::<IDispatch>().map(Object).map_err(|e| {
        format!(
            "an object VirtualBox gave can't be called by name: {}",
            said(&e)
        )
    })
}

/// Runs `read` over a one-dimensional SAFEARRAY's elements, held in place
/// meanwhile.
unsafe fn with_elements<T, R>(
    array: *mut SAFEARRAY,
    read: impl FnOnce(&[T]) -> R,
) -> Result<R, String> {
    if array.is_null() {
        return Ok(read(&[]));
    }
    unsafe {
        let lower = SafeArrayGetLBound(array, 1).map_err(|e| said(&e))?;
        let upper = SafeArrayGetUBound(array, 1).map_err(|e| said(&e))?;
        let count = (upper - lower + 1).max(0) as usize;
        // Rust takes no null pointer even for no elements, and Windows
        // doesn't say what an empty array's data pointer is.
        if count == 0 {
            return Ok(read(&[]));
        }
        let mut data = null_mut();
        SafeArrayAccessData(array, &mut data).map_err(|e| said(&e))?;
        let result = read(std::slice::from_raw_parts(data as *const T, count));
        let _ = SafeArrayUnaccessData(array);
        Ok(result)
    }
}

fn unexpected(wanted: &str, got: VARENUM) -> String {
    format!(
        "VirtualBox gave a value of type {:#x} where {wanted} was expected",
        got.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Ole::SafeArrayCreateVector;

    #[test]
    fn an_empty_array_reads_as_nothing() {
        let mut v = VARIANT::default();
        unsafe {
            let inner = &mut *v.Anonymous.Anonymous;
            inner.vt = VARENUM(VT_ARRAY.0 | VT_UI1.0);
            inner.Anonymous.parray = SafeArrayCreateVector(VT_UI1, 0, 0);
        }
        assert_eq!(Value(v).bytes(), Ok(vec![]));
    }
}
