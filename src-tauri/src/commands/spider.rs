//! Structure Spider commands: start a pointer-chain search, poll progress,
//! page results, filter, and promote a result to the cheat table.

use crate::dto::{SpiderResultDto, SpiderStatusDto};
use crate::format::format_bytes;
use crate::spider;
use crate::state::AppState;
use nemclass_sdk::table::CheatEntry;
use nemclass_sdk::types::FieldKind;
use parking_lot::Mutex;
use tauri::State;

/// Reasonable safety caps so a search can't explode.
const MAX_DEPTH: usize = 8;
const MAX_STRUCT: usize = 0x4000;

fn parse_needle(kind: FieldKind, s: &str) -> Result<f64, String> {
    let s = s.trim();
    if matches!(kind, FieldKind::F32 | FieldKind::F64) {
        return s.parse::<f64>().map_err(|_| "invalid float value".into());
    }
    let v = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i128::from_str_radix(h, 16).map_err(|_| "invalid hex integer")?
    } else {
        s.parse::<i128>().map_err(|_| "invalid integer")?
    };
    Ok(v as f64)
}

/// Starts a spider search from `address` for `value` of `kind`.
#[tauri::command]
pub fn spider_search(
    state: State<'_, Mutex<AppState>>,
    address: u64,
    struct_size: usize,
    alignment: usize,
    depth: usize,
    kind: String,
    value: String,
) -> Result<(), String> {
    let kind = FieldKind::from_kind_string(&kind).ok_or_else(|| format!("unknown kind `{kind}`"))?;
    if FieldKind::size(&kind) == 0 {
        return Err("spider needs a scalar type".into());
    }
    let needle = parse_needle(kind, &value)?;
    let target = state.lock().target.clone().ok_or("not attached")?;
    let handle = spider::start_search(
        target,
        address as usize,
        struct_size.min(MAX_STRUCT).max(1),
        alignment.max(1),
        depth.min(MAX_DEPTH).max(1),
        kind,
        needle,
    );
    state.lock().spider = Some(handle);
    Ok(())
}

/// Whether a search is still running, and how many results so far.
#[tauri::command]
pub fn spider_status(state: State<'_, Mutex<AppState>>) -> Result<SpiderStatusDto, String> {
    let st = state.lock();
    Ok(match &st.spider {
        Some(h) => SpiderStatusDto {
            running: h.running(),
            count: h.results.lock().unwrap().len(),
        },
        None => SpiderStatusDto {
            running: false,
            count: 0,
        },
    })
}

/// A page of spider results with live-resolved addresses/values.
#[tauri::command]
pub fn spider_page(
    state: State<'_, Mutex<AppState>>,
    offset: usize,
    limit: usize,
) -> Result<Vec<SpiderResultDto>, String> {
    let st = state.lock();
    let h = st.spider.as_ref().ok_or("no spider search")?;
    let target = st.target.clone();
    let kind = h.kind;
    let ptr = st.ptr_size();
    let root = h.root;
    let results = h.results.lock().unwrap();
    let end = (offset + limit).min(results.len());

    Ok(results[offset..end]
        .iter()
        .map(|r| {
            let address =
                target.as_ref().and_then(|t| spider::resolve(t, root, r));
            let value = address
                .and_then(|a| target.as_ref().and_then(|t| t.read_bytes(a, kind.size_with_ptr(ptr))))
                .map(|b| format_bytes(kind, &b, ptr));
            SpiderResultDto {
                expr: spider::expr(root, r),
                depth: r.parent_offsets.len(),
                address: address.map(|a| a as u64),
                value,
            }
        })
        .collect())
}

/// Filters the current results by comparing each against `value`.
#[tauri::command]
pub fn spider_filter(
    state: State<'_, Mutex<AppState>>,
    filter: String,
    value: String,
) -> Result<usize, String> {
    let st = state.lock();
    let h = st.spider.as_ref().ok_or("no spider search")?;
    let target = st.target.clone().ok_or("not attached")?;
    let kind = h.kind;
    let root = h.root;
    let needle = parse_needle(kind, &value).unwrap_or(0.0);
    let mut results = h.results.lock().unwrap();
    results.retain_mut(|r| spider::should_remain(&target, root, r, kind, &filter, needle));
    Ok(results.len())
}

/// Cancels the current search.
#[tauri::command]
pub fn spider_cancel(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    if let Some(h) = &state.lock().spider {
        h.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    Ok(())
}

/// Adds a spider result to the cheat table using its pointer-chain expression.
#[tauri::command]
pub fn spider_add_to_table(
    state: State<'_, Mutex<AppState>>,
    index: usize,
    description: String,
) -> Result<(), String> {
    let mut st = state.lock();
    let (expr, kind) = {
        let h = st.spider.as_ref().ok_or("no spider search")?;
        let results = h.results.lock().unwrap();
        let r = results.get(index).ok_or("index out of range")?;
        (spider::expr(h.root, r), h.kind)
    };
    let desc = if description.is_empty() {
        "spider".to_string()
    } else {
        description
    };
    st.cheat_table.entries.push(CheatEntry::new(desc, expr, kind));
    Ok(())
}
