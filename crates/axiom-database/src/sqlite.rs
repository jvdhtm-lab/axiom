//! Minimal SQLite binding.
//!
//! The crate deliberately avoids native build scripts and external crates.
//! SQLite is loaded at runtime (`winsqlite3.dll` on Windows, `libsqlite3` on
//! Unix) so the offline test suite needs no network and no bundled C build.

use std::ffi::{c_char, c_int, c_uchar, c_void, CStr, CString};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::value::{Params, Row, Value};
use crate::DbError;

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_DONE: c_int = 101;
const SQLITE_READONLY: c_int = 8;
const SQLITE_INTERRUPT: c_int = 9;
const SQLITE_BUSY: c_int = 5;
const SQLITE_LOCKED: c_int = 6;

const OPEN_READONLY: c_int = 0x0000_0001;
const OPEN_READWRITE: c_int = 0x0000_0002;
const OPEN_CREATE: c_int = 0x0000_0004;

type OpenV2 = unsafe extern "C" fn(*const c_char, *mut *mut c_void, c_int, *const c_char) -> c_int;
type CloseV2 = unsafe extern "C" fn(*mut c_void) -> c_int;
type PrepareV2 = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    c_int,
    *mut *mut c_void,
    *mut *const c_char,
) -> c_int;
type Step = unsafe extern "C" fn(*mut c_void) -> c_int;
type Finalize = unsafe extern "C" fn(*mut c_void) -> c_int;
type BindParameterCount = unsafe extern "C" fn(*mut c_void) -> c_int;
type BindParameterName = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;
type BindNull = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type BindInt64 = unsafe extern "C" fn(*mut c_void, c_int, i64) -> c_int;
type BindDouble = unsafe extern "C" fn(*mut c_void, c_int, f64) -> c_int;
type BindText =
    unsafe extern "C" fn(*mut c_void, c_int, *const c_char, c_int, *const c_void) -> c_int;
type BindBlob =
    unsafe extern "C" fn(*mut c_void, c_int, *const c_void, c_int, *const c_void) -> c_int;
type ColumnCount = unsafe extern "C" fn(*mut c_void) -> c_int;
type ColumnType = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type ColumnInt64 = unsafe extern "C" fn(*mut c_void, c_int) -> i64;
type ColumnDouble = unsafe extern "C" fn(*mut c_void, c_int) -> f64;
type ColumnText = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_uchar;
type ColumnBlob = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_void;
type ColumnBytes = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type ColumnName = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;
type Changes = unsafe extern "C" fn(*mut c_void) -> c_int;
type BusyTimeout = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type ProgressCallback = unsafe extern "C" fn(*mut c_void) -> c_int;
type ProgressHandler =
    unsafe extern "C" fn(*mut c_void, c_int, Option<ProgressCallback>, *mut c_void);
type ExtendedErrcode = unsafe extern "C" fn(*mut c_void) -> c_int;

const SQLITE_TRANSIENT: *const c_void = usize::MAX as *const c_void;

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::{c_char, c_void, CString};

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *const c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }

    pub struct Handle(*mut c_void);

    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    impl Handle {
        pub fn open() -> Option<Handle> {
            let name = CString::new("winsqlite3.dll").ok()?;
            let handle = unsafe { LoadLibraryA(name.as_ptr()) };
            if handle.is_null() {
                None
            } else {
                Some(Handle(handle))
            }
        }

        pub fn sym(&self, name: &str) -> Option<*const c_void> {
            let name = CString::new(name).ok()?;
            let symbol = unsafe { GetProcAddress(self.0, name.as_ptr()) };
            if symbol.is_null() {
                None
            } else {
                Some(symbol)
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
}

#[cfg(unix)]
mod platform {
    use std::ffi::{c_char, c_void, CString};

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))]
    #[link(name = "dl")]
    extern "C" {
        fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *const c_void;
        fn dlclose(handle: *mut c_void) -> i32;
    }

    #[cfg(target_os = "macos")]
    extern "C" {
        fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *const c_void;
        fn dlclose(handle: *mut c_void) -> i32;
    }

    const RTLD_NOW: i32 = 2;

    pub struct Handle(*mut c_void);

    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    impl Handle {
        pub fn open() -> Option<Handle> {
            for name in ["libsqlite3.so.0", "libsqlite3.so", "libsqlite3.dylib"] {
                let name = CString::new(name).ok()?;
                let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW) };
                if !handle.is_null() {
                    return Some(Handle(handle));
                }
            }
            None
        }

        pub fn sym(&self, name: &str) -> Option<*const c_void> {
            let name = CString::new(name).ok()?;
            let symbol = unsafe { dlsym(self.0, name.as_ptr()) };
            if symbol.is_null() {
                None
            } else {
                Some(symbol)
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                dlclose(self.0);
            }
        }
    }
}

#[cfg(not(any(target_os = "windows", unix)))]
mod platform {
    pub struct Handle;

    impl Handle {
        pub fn open() -> Option<Handle> {
            None
        }

        pub fn sym(&self, _name: &str) -> Option<*const std::ffi::c_void> {
            None
        }
    }
}

struct Api {
    _handle: platform::Handle,
    open_v2: OpenV2,
    close_v2: CloseV2,
    prepare_v2: PrepareV2,
    step: Step,
    finalize: Finalize,
    bind_parameter_count: BindParameterCount,
    bind_parameter_name: BindParameterName,
    bind_null: BindNull,
    bind_int64: BindInt64,
    bind_double: BindDouble,
    bind_text: BindText,
    bind_blob: BindBlob,
    column_count: ColumnCount,
    column_type: ColumnType,
    column_int64: ColumnInt64,
    column_double: ColumnDouble,
    column_text: ColumnText,
    column_blob: ColumnBlob,
    column_bytes: ColumnBytes,
    column_name: ColumnName,
    changes: Changes,
    busy_timeout: BusyTimeout,
    progress_handler: ProgressHandler,
    extended_errcode: ExtendedErrcode,
}

static API: OnceLock<Option<Arc<Api>>> = OnceLock::new();

impl Api {
    fn get() -> Result<Arc<Api>, DbError> {
        let cached = API.get_or_init(|| load_api().map(Arc::new));
        cached.clone().ok_or(DbError::Unavailable("SQLite runtime"))
    }

    fn code(&self, db: *mut c_void) -> i32 {
        unsafe { (self.extended_errcode)(db) }
    }
}

fn load_api() -> Option<Api> {
    let handle = platform::Handle::open()?;
    macro_rules! symbol {
        ($name:literal, $ty:ty) => {{
            let pointer = handle.sym($name)?;
            unsafe { std::mem::transmute::<*const c_void, $ty>(pointer) }
        }};
    }
    Some(Api {
        open_v2: symbol!("sqlite3_open_v2", OpenV2),
        close_v2: symbol!("sqlite3_close_v2", CloseV2),
        prepare_v2: symbol!("sqlite3_prepare_v2", PrepareV2),
        step: symbol!("sqlite3_step", Step),
        finalize: symbol!("sqlite3_finalize", Finalize),
        bind_parameter_count: symbol!("sqlite3_bind_parameter_count", BindParameterCount),
        bind_parameter_name: symbol!("sqlite3_bind_parameter_name", BindParameterName),
        bind_null: symbol!("sqlite3_bind_null", BindNull),
        bind_int64: symbol!("sqlite3_bind_int64", BindInt64),
        bind_double: symbol!("sqlite3_bind_double", BindDouble),
        bind_text: symbol!("sqlite3_bind_text", BindText),
        bind_blob: symbol!("sqlite3_bind_blob", BindBlob),
        column_count: symbol!("sqlite3_column_count", ColumnCount),
        column_type: symbol!("sqlite3_column_type", ColumnType),
        column_int64: symbol!("sqlite3_column_int64", ColumnInt64),
        column_double: symbol!("sqlite3_column_double", ColumnDouble),
        column_text: symbol!("sqlite3_column_text", ColumnText),
        column_blob: symbol!("sqlite3_column_blob", ColumnBlob),
        column_bytes: symbol!("sqlite3_column_bytes", ColumnBytes),
        column_name: symbol!("sqlite3_column_name", ColumnName),
        changes: symbol!("sqlite3_changes", Changes),
        busy_timeout: symbol!("sqlite3_busy_timeout", BusyTimeout),
        progress_handler: symbol!("sqlite3_progress_handler", ProgressHandler),
        extended_errcode: symbol!("sqlite3_extended_errcode", ExtendedErrcode),
        _handle: handle,
    })
}

/// A SQLite connection. Not `Clone`; each role owns its own connection.
pub struct Connection {
    api: Arc<Api>,
    db: *mut c_void,
}

unsafe impl Send for Connection {}

impl Connection {
    /// Open a connection. `read_only` uses SQLite's read-only flag.
    pub fn open(path: &Path, read_only: bool) -> Result<Self, DbError> {
        let api = Api::get()?;
        let text = path.to_string_lossy().to_string();
        let c_path = CString::new(text).map_err(|_| DbError::Config("path contains NUL".into()))?;
        let flags = if read_only {
            OPEN_READONLY
        } else {
            OPEN_READWRITE | OPEN_CREATE
        };
        let mut db: *mut c_void = std::ptr::null_mut();
        let code = unsafe { (api.open_v2)(c_path.as_ptr(), &mut db, flags, std::ptr::null()) };
        if code != SQLITE_OK {
            let code = if db.is_null() { code } else { api.code(db) };
            if !db.is_null() {
                unsafe {
                    (api.close_v2)(db);
                }
            }
            return Err(map_code(code));
        }
        unsafe {
            (api.busy_timeout)(db, 2_000);
        }
        Ok(Connection { api, db })
    }

    /// Execute one or more statements with no parameters. Used by migrations.
    pub fn script(&self, sql: &str) -> Result<(), DbError> {
        let c_sql = CString::new(sql).map_err(|_| DbError::InvalidSql)?;
        let mut rest = c_sql.as_ptr();
        loop {
            let mut statement: *mut c_void = std::ptr::null_mut();
            let mut tail: *const c_char = std::ptr::null();
            let code =
                unsafe { (self.api.prepare_v2)(self.db, rest, -1, &mut statement, &mut tail) };
            if code != SQLITE_OK {
                return Err(self.error(code));
            }
            if statement.is_null() {
                break;
            }
            let mut step = unsafe { (self.api.step)(statement) };
            while step == SQLITE_ROW {
                step = unsafe { (self.api.step)(statement) };
            }
            let finalize = unsafe { (self.api.finalize)(statement) };
            if step != SQLITE_DONE {
                return Err(map_code(step));
            }
            if finalize != SQLITE_OK {
                return Err(self.error(finalize));
            }
            if tail.is_null() || tail == rest {
                break;
            }
            rest = tail;
        }
        Ok(())
    }

    /// Prepare and execute a read statement, returning at most `max_rows`.
    pub fn query(
        &self,
        sql: &str,
        params: &Params,
        max_rows: u64,
        timeout: Duration,
    ) -> Result<Vec<Row>, DbError> {
        let mut statement = self.prepare(sql)?;
        self.bind(&statement, params)?;
        let deadline = Box::new(Deadline(Instant::now() + timeout));
        unsafe {
            (self.api.progress_handler)(
                self.db,
                1_000,
                Some(progress),
                &*deadline as *const Deadline as *mut c_void,
            );
        }
        let mut rows = Vec::new();
        loop {
            let code = unsafe { (self.api.step)(statement.raw) };
            match code {
                SQLITE_ROW => {
                    if rows.len() as u64 >= max_rows {
                        unsafe {
                            (self.api.progress_handler)(self.db, 0, None, std::ptr::null_mut());
                            statement.finish();
                        }
                        return Err(DbError::RowLimitExceeded { max_rows });
                    }
                    rows.push(self.read_row(statement.raw));
                }
                SQLITE_DONE => break,
                SQLITE_INTERRUPT => {
                    unsafe {
                        (self.api.progress_handler)(self.db, 0, None, std::ptr::null_mut());
                        statement.finish();
                    }
                    return Err(DbError::Timeout);
                }
                other => {
                    unsafe {
                        (self.api.progress_handler)(self.db, 0, None, std::ptr::null_mut());
                        statement.finish();
                    }
                    return Err(self.error(other));
                }
            }
        }
        unsafe {
            (self.api.progress_handler)(self.db, 0, None, std::ptr::null_mut());
            statement.finish();
        }
        Ok(rows)
    }

    /// Execute a single write statement and return the affected row count.
    pub fn execute(&self, sql: &str, params: &Params, timeout: Duration) -> Result<usize, DbError> {
        let mut statement = self.prepare(sql)?;
        self.bind(&statement, params)?;
        let deadline = Box::new(Deadline(Instant::now() + timeout));
        unsafe {
            (self.api.progress_handler)(
                self.db,
                1_000,
                Some(progress),
                &*deadline as *const Deadline as *mut c_void,
            );
        }
        loop {
            let code = unsafe { (self.api.step)(statement.raw) };
            match code {
                SQLITE_ROW => {}
                SQLITE_DONE => break,
                SQLITE_INTERRUPT => {
                    unsafe {
                        (self.api.progress_handler)(self.db, 0, None, std::ptr::null_mut());
                        statement.finish();
                    }
                    return Err(DbError::Timeout);
                }
                other => {
                    unsafe {
                        (self.api.progress_handler)(self.db, 0, None, std::ptr::null_mut());
                        statement.finish();
                    }
                    return Err(self.error(other));
                }
            }
        }
        let changed = unsafe { (self.api.changes)(self.db) };
        unsafe {
            (self.api.progress_handler)(self.db, 0, None, std::ptr::null_mut());
            statement.finish();
        }
        Ok(changed.max(0) as usize)
    }

    /// Run `BEGIN IMMEDIATE`.
    pub fn begin_immediate(&self) -> Result<(), DbError> {
        self.script("BEGIN IMMEDIATE")
    }

    /// Run `COMMIT`.
    pub fn commit(&self) -> Result<(), DbError> {
        self.script("COMMIT")
    }

    /// Run `ROLLBACK`; failure is reported but usually means no transaction.
    pub fn rollback(&self) -> Result<(), DbError> {
        self.script("ROLLBACK")
    }

    fn prepare(&self, sql: &str) -> Result<Statement, DbError> {
        let c_sql = CString::new(sql).map_err(|_| DbError::InvalidSql)?;
        let mut statement: *mut c_void = std::ptr::null_mut();
        let mut tail: *const c_char = std::ptr::null();
        let code = unsafe {
            (self.api.prepare_v2)(self.db, c_sql.as_ptr(), -1, &mut statement, &mut tail)
        };
        if code != SQLITE_OK {
            return Err(self.error(code));
        }
        if !tail.is_null() {
            let mut cursor = tail;
            loop {
                let byte = unsafe { *cursor } as u8;
                if byte == 0 {
                    break;
                }
                if byte == b';' || (byte as char).is_ascii_whitespace() {
                    cursor = unsafe { cursor.add(1) };
                    continue;
                }
                unsafe {
                    (self.api.finalize)(statement);
                }
                return Err(DbError::MultipleStatements);
            }
        }
        Ok(Statement {
            api: self.api.clone(),
            raw: statement,
        })
    }

    fn bind(&self, statement: &Statement, params: &Params) -> Result<(), DbError> {
        let count = unsafe { (self.api.bind_parameter_count)(statement.raw) };
        for index in 1..=count {
            let name = unsafe { (self.api.bind_parameter_name)(statement.raw, index) };
            if name.is_null() {
                return Err(DbError::PlaceholderMismatch(
                    "anonymous placeholder".to_string(),
                ));
            }
            let name = unsafe { CStr::from_ptr(name) }
                .to_string_lossy()
                .trim_start_matches([':', '@', '$'])
                .to_string();
            let Some(value) = params.get(&name) else {
                return Err(DbError::PlaceholderMismatch(format!(
                    "missing parameter `{name}`"
                )));
            };
            let code = match value {
                Value::Null => unsafe { (self.api.bind_null)(statement.raw, index) },
                Value::Integer(value) => unsafe {
                    (self.api.bind_int64)(statement.raw, index, *value)
                },
                Value::Real(value) => unsafe {
                    (self.api.bind_double)(statement.raw, index, *value)
                },
                Value::Text(value) => unsafe {
                    (self.api.bind_text)(
                        statement.raw,
                        index,
                        value.as_ptr() as *const c_char,
                        value.len() as c_int,
                        SQLITE_TRANSIENT,
                    )
                },
                Value::Blob(value) => unsafe {
                    (self.api.bind_blob)(
                        statement.raw,
                        index,
                        value.as_ptr() as *const c_void,
                        value.len() as c_int,
                        SQLITE_TRANSIENT,
                    )
                },
            };
            if code != SQLITE_OK {
                return Err(DbError::Engine { code });
            }
        }
        Ok(())
    }

    fn read_row(&self, statement: *mut c_void) -> Row {
        let count = unsafe { (self.api.column_count)(statement) };
        let mut columns = Vec::with_capacity(count.max(0) as usize);
        for index in 0..count {
            let name = unsafe { (self.api.column_name)(statement, index) };
            let name = if name.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(name) }
                    .to_string_lossy()
                    .to_string()
            };
            let value = match unsafe { (self.api.column_type)(statement, index) } {
                1 => Value::Integer(unsafe { (self.api.column_int64)(statement, index) }),
                2 => Value::Real(unsafe { (self.api.column_double)(statement, index) }),
                3 => {
                    let text = unsafe { (self.api.column_text)(statement, index) };
                    let bytes = unsafe { (self.api.column_bytes)(statement, index) };
                    if text.is_null() || bytes <= 0 {
                        Value::Text(String::new())
                    } else {
                        let slice = unsafe {
                            std::slice::from_raw_parts(text, bytes as usize)
                        };
                        Value::Text(String::from_utf8_lossy(slice).to_string())
                    }
                }
                4 => {
                    let blob = unsafe { (self.api.column_blob)(statement, index) };
                    let bytes = unsafe { (self.api.column_bytes)(statement, index) };
                    if blob.is_null() || bytes <= 0 {
                        Value::Blob(Vec::new())
                    } else {
                        let slice = unsafe {
                            std::slice::from_raw_parts(blob as *const u8, bytes as usize)
                        };
                        Value::Blob(slice.to_vec())
                    }
                }
                _ => Value::Null,
            };
            columns.push((name, value));
        }
        Row::new(columns)
    }

    fn error(&self, code: c_int) -> DbError {
        let extended = if self.db.is_null() {
            code
        } else {
            let detail = self.api.code(self.db);
            if detail == SQLITE_OK {
                code
            } else {
                detail
            }
        };
        let base = extended & 0xff;
        match base {
            SQLITE_BUSY | SQLITE_LOCKED => DbError::Busy,
            SQLITE_INTERRUPT => DbError::Timeout,
            SQLITE_READONLY => DbError::RoleDenied {
                role: "read",
                operation: "write",
            },
            _ => DbError::Engine { code: extended },
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            (self.api.close_v2)(self.db);
        }
    }
}

fn map_code(code: c_int) -> DbError {
    match code & 0xff {
        SQLITE_BUSY | SQLITE_LOCKED => DbError::Busy,
        SQLITE_INTERRUPT => DbError::Timeout,
        SQLITE_READONLY => DbError::RoleDenied {
            role: "read",
            operation: "write",
        },
        _ => DbError::Engine { code },
    }
}

struct Statement {
    api: Arc<Api>,
    raw: *mut c_void,
}

impl Statement {
    fn finish(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                (self.api.finalize)(self.raw);
            }
            self.raw = std::ptr::null_mut();
        }
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        self.finish();
    }
}

struct Deadline(Instant);

unsafe extern "C" fn progress(arg: *mut c_void) -> c_int {
    if arg.is_null() {
        return 0;
    }
    let deadline = unsafe { &*(arg as *const Deadline) };
    if Instant::now() >= deadline.0 {
        1
    } else {
        0
    }
}
