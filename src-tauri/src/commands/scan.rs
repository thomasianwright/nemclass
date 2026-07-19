//! CheatEngine-style value scanner. First/next scans run off the command thread
//! via `spawn_blocking`; the result set lives in [`AppState`] for refinement.

use crate::dto::{CompareDto, ScanRowDto, ScanSummaryDto};
use crate::format::format_bytes;
use crate::state::AppState;
use nemclass_sdk::scan::{ScanCompare, ScanConfig, ScanType, ScanValue, Scanner};
use nemclass_sdk::table::CheatEntry;
use parking_lot::Mutex;
use tauri::State;

fn parse_scan_type(s: &str) -> Result<ScanType, String> {
    Ok(match s {
        "I8" => ScanType::I8,
        "U8" => ScanType::U8,
        "I16" => ScanType::I16,
        "U16" => ScanType::U16,
        "I32" => ScanType::I32,
        "U32" => ScanType::U32,
        "I64" => ScanType::I64,
        "U64" => ScanType::U64,
        "F32" => ScanType::F32,
        "F64" => ScanType::F64,
        "Bytes" => ScanType::Bytes,
        _ => return Err(format!("unknown scan type `{s}`")),
    })
}

fn parse_scan_value(ty: ScanType, s: &str) -> Result<ScanValue, String> {
    let s = s.trim();
    if ty == ScanType::Bytes {
        let bytes: Result<Vec<u8>, _> = s
            .split_whitespace()
            .map(|b| u8::from_str_radix(b.trim_start_matches("0x"), 16))
            .collect();
        return bytes.map(ScanValue::Bytes).map_err(|_| "invalid byte pattern".into());
    }
    if ty.is_float() {
        return s.parse::<f64>().map(ScanValue::Float).map_err(|_| "invalid float".into());
    }
    let v = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i128::from_str_radix(h, 16).map_err(|_| "invalid hex integer")?
    } else {
        s.parse::<i128>().map_err(|_| "invalid integer")?
    };
    Ok(ScanValue::Int(v))
}

impl CompareDto {
    fn into_compare(self, ty: ScanType) -> Result<ScanCompare, String> {
        let v = |o: Option<String>| -> Result<ScanValue, String> {
            parse_scan_value(ty, &o.ok_or("comparison needs a value")?)
        };
        Ok(match self.op.as_str() {
            "exact" => ScanCompare::Exact(v(self.value)?),
            "unknown" => ScanCompare::Unknown,
            "between" => ScanCompare::Between(v(self.value)?, v(self.value2)?),
            "greater" => ScanCompare::Greater(v(self.value)?),
            "less" => ScanCompare::Less(v(self.value)?),
            "increased" => ScanCompare::Increased,
            "decreased" => ScanCompare::Decreased,
            "changed" => ScanCompare::Changed,
            "unchanged" => ScanCompare::Unchanged,
            "increasedBy" => ScanCompare::IncreasedBy(v(self.value)?),
            "decreasedBy" => ScanCompare::DecreasedBy(v(self.value)?),
            other => return Err(format!("unknown comparison `{other}`")),
        })
    }
}

/// Runs a first scan over the target's readable regions.
#[tauri::command]
pub async fn scan_first(
    state: State<'_, Mutex<AppState>>,
    value_type: String,
    compare: CompareDto,
    writable_only: bool,
    alignment: Option<usize>,
) -> Result<ScanSummaryDto, String> {
    let target = state.lock().target.clone().ok_or("not attached")?;
    let ty = parse_scan_type(&value_type)?;
    let mut cfg = ScanConfig::new(ty);
    cfg.writable_only = writable_only;
    if let Some(a) = alignment {
        cfg.alignment = a.max(1);
    }
    let cmp = compare.into_compare(ty)?;

    let results = tauri::async_runtime::spawn_blocking(move || {
        Scanner::new(&target, cfg).first_scan(cmp)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let summary = ScanSummaryDto {
        count: results.len(),
        value_type: value_type.clone(),
    };
    state.lock().scan_results = Some(results);
    Ok(summary)
}

/// Refines the previous scan with a new comparison.
#[tauri::command]
pub async fn scan_next(
    state: State<'_, Mutex<AppState>>,
    compare: CompareDto,
) -> Result<ScanSummaryDto, String> {
    let (target, prev) = {
        let st = state.lock();
        (st.target.clone(), st.scan_results.clone())
    };
    let target = target.ok_or("not attached")?;
    let prev = prev.ok_or("no previous scan — run a first scan")?;
    let ty = prev.value_type();
    let cmp = compare.into_compare(ty)?;
    let cfg = ScanConfig::new(ty);

    let results = tauri::async_runtime::spawn_blocking(move || {
        Scanner::new(&target, cfg).next_scan(&prev, cmp)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let summary = ScanSummaryDto {
        count: results.len(),
        value_type: format!("{ty:?}"),
    };
    state.lock().scan_results = Some(results);
    Ok(summary)
}

/// Clears the current scan.
#[tauri::command]
pub fn scan_reset(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    state.lock().scan_results = None;
    Ok(())
}

/// A page of scan results with live + snapshot values.
#[tauri::command]
pub fn scan_page(
    state: State<'_, Mutex<AppState>>,
    offset: usize,
    limit: usize,
) -> Result<Vec<ScanRowDto>, String> {
    let st = state.lock();
    let results = st.scan_results.as_ref().ok_or("no scan")?;
    let ty = results.value_type();
    let kind = ty.to_field_kind();
    let ptr = st.ptr_size();
    let target = st.target.clone();
    let addrs = results.addresses();
    let end = (offset + limit).min(addrs.len());

    let fmt = |b: &[u8]| match kind {
        Some(k) => format_bytes(k, b, ptr),
        None => b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" "),
    };

    Ok((offset..end)
        .map(|i| {
            let addr = addrs[i];
            let previous = results.value_bytes(i).map(fmt).unwrap_or_default();
            let size = ty.size().max(results.value_bytes(i).map_or(0, |b| b.len()));
            let value = target
                .as_ref()
                .and_then(|t| t.read_bytes(addr, size))
                .map(|b| fmt(&b));
            ScanRowDto {
                address: addr as u64,
                value,
                previous,
            }
        })
        .collect())
}

/// Adds a scan result to the cheat table.
#[tauri::command]
pub fn scan_add_to_table(
    state: State<'_, Mutex<AppState>>,
    index: usize,
    description: String,
) -> Result<(), String> {
    let mut st = state.lock();
    let (addr, kind) = {
        let results = st.scan_results.as_ref().ok_or("no scan")?;
        let addr = results
            .addresses()
            .get(index)
            .copied()
            .ok_or("index out of range")?;
        let kind = results
            .value_type()
            .to_field_kind()
            .ok_or("byte-array scans can't be added as a typed entry")?;
        (addr, kind)
    };
    let desc = if description.is_empty() {
        format!("0x{addr:X}")
    } else {
        description
    };
    st.cheat_table
        .entries
        .push(CheatEntry::new(desc, format!("0x{addr:X}"), kind));
    Ok(())
}
