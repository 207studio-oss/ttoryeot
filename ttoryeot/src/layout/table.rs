//! 표: 열 너비·행 높이, 칸 그리기, 쪽에서 나뉜 표.

use super::object::{Edge, LineGrid, border_items, frame_items};
use super::record::Flags;
use super::{Renderer, page_scale, pt};
use crate::components::Kind;
use crate::scene::{Item, Rect};
use hwp_core::{Cell, Paragraph, Section, Table};
use std::collections::{BTreeSet, HashMap, HashSet};

/// 칸 글의 문단 번호 → 그릴 줄 번호. 없는 문단은 그리지 않는다.
type Lines = HashMap<usize, BTreeSet<usize>>;

/// 칸 글의 줄을 쪽 조각으로 나눈다. 한/글은 나뉜 칸의 다음 조각 줄 위치를 그 조각의 칸 위에서 다시 세므로
/// 앞줄보다 위로 가거나 같은 줄이 나오면 새 조각이다. 조각마다 (문단 번호, 줄 번호)의 차례.
fn cell_fragments(cell: &Cell) -> Vec<Vec<(usize, usize)>> {
    let mut out = vec![Vec::new()];
    let mut last = -1.0;
    let mut started = false;
    for (pi, para) in cell.paragraphs.iter().enumerate() {
        for (li, seg) in para.line_segs.iter().enumerate() {
            if started && seg.vertpos <= last + 0.5 {
                out.push(Vec::new());
            }
            started = true;
            last = seg.vertpos;
            if let Some(fragment) = out.last_mut() {
                fragment.push((pi, li));
            }
        }
    }
    out
}

/// 나뉘지 않는 표의 칸 글에서 문단마다 저장된 줄 위치에 더할 값(HWPUNIT). 한/글이 아닌 도구가 쓴 문서에는 칸 속 문단의
/// 줄 위치가 앞 문단과 겹치게(모두 0으로) 저장된 것이 있는데, 한/글은 그 문단을 앞 문단 아래(줄 간격까지)에 다시 쌓는다.
/// 줄 간격이 음수라 겹치는 것은 저장된 자리 그대로이고, 1pt 이하의 차이는 반올림으로 본다.
fn stack_offsets(cell: &Cell) -> Vec<f64> {
    let mut offsets = Vec::with_capacity(cell.paragraphs.len());
    let mut floor: Option<f64> = None;
    let mut offset = 0.0;
    for para in &cell.paragraphs {
        if let (Some(first), Some(last)) = (para.line_segs.first(), para.line_segs.last()) {
            let needed = floor.map_or(0.0, |floor| floor - first.vertpos);
            offset = if needed > FIT_TOLERANCE { needed } else { 0.0 };
            floor = Some(last.vertpos + offset + last.vertsize + last.spacing);
        }
        offsets.push(offset);
    }
    offsets
}

/// 칸 폭에서 좌우 여백을 뺀 내용 폭(HWPUNIT).
fn content_width(cell: &Cell, columns: &[f64], spacing: f64) -> f64 {
    let margins = cell.margins.left + cell.margins.right;
    match index(cell.col, columns.len()) {
        Some(col) => {
            let end = columns.len().min(col + cell.col_span.max(1) as usize);
            (columns[col..end].iter().sum::<f64>() + (end - col).saturating_sub(1) as f64 * spacing
                - margins)
                .max(0.0)
        }
        None => (cell.width - margins).max(0.0),
    }
}

/// 행이 쪽 끝에 들어가는지 볼 때 허용하는 차이(HWPUNIT 1pt).
const FIT_TOLERANCE: f64 = 100.0;
/// 걸친 칸의 남는 높이를 다시 세는 횟수. 조각 높이가 바뀌면 다시 짓고, 보통 두 번이면 멈춘다.
const SETTLE_PASSES: usize = 3;

fn spacing(table: &Table) -> f64 {
    table.cell_spacing.max(0.0)
}

fn row_count(table: &Table) -> usize {
    table.rows.max(1) as usize
}

fn index(value: i32, len: usize) -> Option<usize> {
    usize::try_from(value).ok().filter(|value| *value < len)
}

/// `row` 행에서 세로 칸 경계 `boundary`에 그리는 세로 변 가운데 가장 굵은 것.
fn vertical_at<'f>(
    table: &Table,
    fills: &HashMap<i32, &'f hwp_core::BorderFill>,
    row: usize,
    boundary: usize,
) -> Option<&'f hwp_core::Border> {
    table
        .cells
        .iter()
        .filter(|cell| {
            let start = cell.row.max(0) as usize;
            (start..start + cell.row_span.max(1) as usize).contains(&row)
        })
        .filter_map(|cell| {
            let fill = fills.get(&cell.border_fill_id)?;
            let start = cell.col.max(0) as usize;
            let border = if start + cell.col_span.max(1) as usize == boundary {
                &fill.right
            } else if start == boundary {
                &fill.left
            } else {
                return None;
            };
            (border.border_type != "NONE").then_some(border)
        })
        .max_by(|a, b| a.width.total_cmp(&b.width))
}

/// `cell`의 모서리(`row` 0 위·1 아래, `col` 0 왼·1 오른)를 가로지르는 이웃 칸 쪽 가로 겹선. 세로 변 너머 칸들이
/// 그 행 경계에 둔 변 가운데 가장 굵은 것이 겹선이고, 이 칸 쪽(이 칸과 그 위·아래 칸)의 가로 변보다 굵을 때만 준다.
fn crossing_double<'f>(
    table: &Table,
    fills: &HashMap<i32, &'f hwp_core::BorderFill>,
    cell: &Cell,
    row: usize,
    col: usize,
) -> Option<&'f hwp_core::Border> {
    let line = if row == 0 {
        cell.row
    } else {
        cell.row + cell.row_span.max(1)
    };
    let boundary = if col == 0 {
        cell.col
    } else {
        cell.col + cell.col_span.max(1)
    };
    // 행 경계 `line`에 닿은 칸들의 가로 변. `beyond`가 참이면 세로 변 너머, 거짓이면 이 칸 쪽.
    let edges = |beyond: bool| {
        table.cells.iter().filter_map(move |other| {
            let (start, end) = (other.col, other.col + other.col_span.max(1));
            let side = if beyond == (col == 1) {
                start == boundary
            } else {
                end == boundary
            };
            if !side {
                return None;
            }
            let fill = fills.get(&other.border_fill_id)?;
            let border = if other.row + other.row_span.max(1) == line {
                &fill.bottom
            } else if other.row == line {
                &fill.top
            } else {
                return None;
            };
            (border.border_type != "NONE").then_some(border)
        })
    };
    let cross = edges(true).max_by(|a, b| a.width.total_cmp(&b.width))?;
    let own = edges(false).map(|border| border.width).fold(0.0, f64::max);
    (cross.border_type.starts_with("DOUBLE") && cross.width > own).then_some(cross)
}

/// 칸의 변이 모서리 너머 이웃 칸의 같은 변으로 같은 선으로 이어지는가.
/// 가로(`vertical` 거짓): 위·아래 변이 왼쪽·오른쪽으로, [[위 왼, 위 오], [아래 왼, 아래 오]].
/// 세로(`vertical` 참): 왼쪽·오른쪽 변이 위·아래로, 같은 꼴([[위 왼, 위 오], [아래 왼, 아래 오]]).
/// `cell` 왼쪽에 붙어 위 모서리(`bottom`이 거짓) 또는 아래 모서리를 같은 자리에 둔 칸.
fn left_neighbor<'t>(table: &'t Table, cell: &Cell, bottom: bool) -> Option<&'t Cell> {
    let edge = |c: &Cell| {
        if bottom {
            c.row + c.row_span.max(1)
        } else {
            c.row
        }
    };
    table
        .cells
        .iter()
        .find(|other| other.col + other.col_span.max(1) == cell.col && edge(other) == edge(cell))
}

fn border_joins(
    table: &Table,
    cell: &Cell,
    fills: &HashMap<i32, &hwp_core::BorderFill>,
    vertical: bool,
) -> [[bool; 2]; 2] {
    // (변을 따라 가는 축, 변에 수직인 축)의 (시작, 칸 수).
    let spans = |cell: &Cell| {
        let rows = (cell.row, cell.row_span.max(1));
        let cols = (cell.col, cell.col_span.max(1));
        if vertical { (rows, cols) } else { (cols, rows) }
    };
    let continues = |low: bool, forward: bool| {
        let ((along, along_span), (across, across_span)) = spans(cell);
        let next = if forward {
            along + along_span
        } else {
            along - 1
        };
        let line = if low {
            across
        } else {
            across + across_span - 1
        };
        let side = |fill: &hwp_core::BorderFill| {
            let border = match (vertical, low) {
                (false, true) => &fill.top,
                (false, false) => &fill.bottom,
                (true, true) => &fill.left,
                (true, false) => &fill.right,
            };
            (border.border_type.clone(), border.width)
        };
        let Some(own) = fills.get(&cell.border_fill_id).map(|fill| side(fill)) else {
            return false;
        };
        table.cells.iter().any(|other| {
            let ((o_along, o_along_span), (o_across, o_across_span)) = spans(other);
            let edge = if low {
                o_across == line
            } else {
                o_across + o_across_span - 1 == line
            };
            (o_along..o_along + o_along_span).contains(&next)
                && (o_across..o_across + o_across_span).contains(&line)
                && edge
                && fills
                    .get(&other.border_fill_id)
                    .is_some_and(|fill| side(fill) == own)
        })
    };
    if vertical {
        // low: 왼쪽 변, forward: 아래로.
        [
            [continues(true, false), continues(false, false)],
            [continues(true, true), continues(false, true)],
        ]
    } else {
        // low: 위 변, forward: 오른쪽으로.
        [
            [continues(true, false), continues(true, true)],
            [continues(false, false), continues(false, true)],
        ]
    }
}
/// 칸 크기 `(시작, 끝, 크기)`들이 잇는 격자 경계(HWPUNIT). 0번 경계는 0이고, 풀 수 없는 경계는 `None`.
fn grid_edges(len: usize, spans: &[(usize, usize, f64)], spacing: f64) -> Vec<Option<f64>> {
    let mut edges: Vec<Option<f64>> = vec![None; len + 1];
    edges[0] = Some(0.0);
    let mut changed = true;
    while changed {
        changed = false;
        for &(start, end, size) in spans {
            match (edges[start], edges[end]) {
                (Some(left), None) => edges[end] = Some(left + size + spacing),
                (None, Some(right)) => edges[start] = Some(right - size - spacing),
                _ => continue,
            }
            changed = true;
        }
    }
    edges
}

/// 행마다 파일에 저장된 높이: 한 행짜리 칸 높이 중 가장 큰 값.
/// `row` 위쪽 경계가 여러 행에 걸친 칸 가운데를 지나는가.
fn spans_across(table: &Table, row: usize) -> bool {
    let row = i64::try_from(row).unwrap_or(i64::MAX);
    table.cells.iter().any(|cell| {
        let start = i64::from(cell.row);
        cell.row_span > 1 && start < row && row < start + i64::from(cell.row_span)
    })
}

fn stored_row_heights(table: &Table) -> Vec<f64> {
    let mut heights = vec![0.0f64; row_count(table)];
    for cell in &table.cells {
        if cell.row_span == 1
            && let Some(row) = index(cell.row, heights.len())
        {
            heights[row] = heights[row].max(cell.height);
        }
    }
    heights
}

/// 한/글이 쪽에서 나눈 표인가: 칸 글의 줄 위치가 되돌아가거나, 저장된 표 높이가 행 높이 합보다 1pt 넘게 작다.
fn is_split_table(table: &Table) -> bool {
    if table.page_break_mode == "NONE" || table.rows <= 0 {
        return false;
    }
    if table
        .cells
        .iter()
        .any(|cell| cell_fragments(cell).len() > 1)
    {
        return true;
    }
    table.height + 100.0
        < stored_row_heights(table).iter().sum::<f64>() + f64::from(table.rows + 1) * spacing(table)
}

/// `row` 행에서 끝나며 `cols`와 겹치는 칸이 있고, 모두 아랫변을 그리는가.
fn header_bottom_drawn(
    table: &Table,
    fills: &HashMap<i32, &hwp_core::BorderFill>,
    row: usize,
    cols: std::ops::Range<usize>,
) -> bool {
    let mut above = table
        .cells
        .iter()
        .filter(|cell| {
            let start = usize::try_from(cell.col).unwrap_or(usize::MAX);
            let end = start.saturating_add(cell.col_span.max(1) as usize);
            let last = i64::from(cell.row) + i64::from(cell.row_span.max(1)) - 1;
            last == row as i64 && start < cols.end && cols.start < end
        })
        .peekable();
    above.peek().is_some()
        && above.all(|cell| {
            fills
                .get(&cell.border_fill_id)
                .is_some_and(|fill| fill.bottom.border_type != "NONE")
        })
}

/// 제목 줄 되풀이가 켜진 표에서 0행부터 이어지는 제목 행.
fn repeated_header_rows(table: &Table) -> Vec<usize> {
    if !table.repeat_header {
        return Vec::new();
    }
    let mut out = Vec::new();
    for row in 0..table.rows.max(0) {
        let starting: Vec<&Cell> = table.cells.iter().filter(|cell| cell.row == row).collect();
        if starting.is_empty()
            || !starting.iter().all(|cell| cell.row_span == 1)
            || !starting.iter().any(|cell| cell.header)
        {
            break;
        }
        out.push(row as usize);
    }
    out
}

impl<'a> Renderer<'a> {
    /// 칸 간격을 뺀 열 너비·행 높이(HWPUNIT). 병합 칸의 최소 높이를 반영한다.
    pub(crate) fn grid(&mut self, table: &'a Table) -> (Vec<f64>, Vec<f64>) {
        let key = table as *const Table as usize;
        if let Some(grid) = self.grids.get(&key) {
            return grid.clone();
        }
        let spacing = spacing(table);
        let mut columns = vec![0.0f64; table.cols.max(1) as usize];
        let mut rows = vec![0.0f64; row_count(table)];
        for cell in &table.cells {
            if cell.col_span == 1
                && let Some(col) = index(cell.col, columns.len())
            {
                columns[col] = columns[col].max(cell.width);
            }
        }
        // 칸 폭은 한 격자를 이룬다. 홑칸이 없는 열은 아는 열과 병합 칸이 잇는 경계로 푼다.
        let spans: Vec<(usize, usize, f64)> = columns
            .iter()
            .enumerate()
            .filter(|(_, width)| **width > 0.0)
            .map(|(col, width)| (col, col + 1, *width))
            .chain(table.cells.iter().filter_map(|cell| {
                let col = index(cell.col, columns.len())?;
                let end = columns.len().min(col + cell.col_span.max(1) as usize);
                (cell.col_span > 1 && cell.width > 0.0).then_some((col, end, cell.width))
            }))
            .collect();
        let edges = grid_edges(columns.len(), &spans, spacing);
        for (col, column) in columns.iter_mut().enumerate() {
            if *column == 0.0
                && let (Some(left), Some(right)) = (edges[col], edges[col + 1])
            {
                *column = (right - left - spacing).max(0.0);
            }
        }
        for cell in &table.cells {
            if cell.col_span > 1
                && let Some(col) = index(cell.col, columns.len())
            {
                let span: Vec<usize> =
                    (col..columns.len().min(col + cell.col_span as usize)).collect();
                let known = span.iter().map(|i| columns[*i]).sum::<f64>()
                    + span.len().saturating_sub(1) as f64 * spacing;
                let zeros: Vec<usize> = span
                    .iter()
                    .copied()
                    .filter(|i| columns[*i] == 0.0)
                    .collect();
                if !zeros.is_empty() {
                    let each = (cell.width - known).max(0.0) / zeros.len() as f64;
                    for i in zeros {
                        columns[i] = each;
                    }
                }
            }
        }
        // 한/글은 표 오른쪽 끝을 저장된 표 너비에 둔다. 너비 합이 표보다 짧은 줄의 홑칸이 열 너비를 정해
        // 열 합이 모자라면, 모자란 만큼 마지막 열을 넓힌다.
        let short =
            table.width - columns.iter().sum::<f64>() - (columns.len() + 1) as f64 * spacing;
        if short > 0.0
            && let Some(last) = columns.last_mut()
        {
            *last += short;
        }
        let widths: Vec<f64> = table
            .cells
            .iter()
            .map(|cell| content_width(cell, &columns, spacing))
            .collect();
        let split_table = is_split_table(table);
        let contents: Vec<f64> = table
            .cells
            .iter()
            .zip(&widths)
            .map(|(cell, width)| {
                if index(cell.col, columns.len()).is_some() && index(cell.row, rows.len()).is_some()
                {
                    self.saved_cell_content_height(cell, None, Some(*width), !split_table)
                } else {
                    0.0
                }
            })
            .collect();
        for (cell, content) in table.cells.iter().zip(&contents) {
            if cell.row_span == 1
                && let Some(row) = index(cell.row, rows.len())
            {
                rows[row] = rows[row].max(
                    cell.height
                        .max(content + cell.margins.top + cell.margins.bottom),
                );
            }
        }
        // 홑칸이 없는 행도 저장된 병합 칸 높이가 잇는 경계로 푼다.
        let spans: Vec<(usize, usize, f64)> = rows
            .iter()
            .enumerate()
            .filter(|(_, height)| **height > 0.0)
            .map(|(row, height)| (row, row + 1, *height))
            .chain(table.cells.iter().filter_map(|cell| {
                let row = index(cell.row, rows.len())?;
                let end = rows.len().min(row + cell.row_span.max(1) as usize);
                (cell.row_span > 1 && cell.height > 0.0).then_some((row, end, cell.height))
            }))
            .collect();
        let edges = grid_edges(rows.len(), &spans, spacing);
        for (row, height) in rows.iter_mut().enumerate() {
            if *height == 0.0
                && let (Some(top), Some(bottom)) = (edges[row], edges[row + 1])
            {
                *height = (bottom - top - spacing).max(0.0);
            }
        }
        for (cell, content) in table.cells.iter().zip(&contents) {
            if cell.row_span > 1
                && let Some(row) = index(cell.row, rows.len())
            {
                let span: Vec<usize> =
                    (row..rows.len().min(row + cell.row_span as usize)).collect();
                let sum = |rows: &[f64]| {
                    span.iter().map(|i| rows[*i]).sum::<f64>()
                        + span.len().saturating_sub(1) as f64 * spacing
                };
                let zeros: Vec<usize> = span.iter().copied().filter(|i| rows[*i] == 0.0).collect();
                if !zeros.is_empty() {
                    let each = (cell.height - sum(&rows)).max(0.0) / zeros.len() as f64;
                    for i in zeros {
                        rows[i] = each;
                    }
                }
                // 나뉜 표는 그린 높이를 알아야 남는 높이를 정할 수 있어 `merged_extra`가 맡는다.
                if !split_table {
                    let need = cell
                        .height
                        .max(content + cell.margins.top + cell.margins.bottom);
                    let have = sum(&rows);
                    if need > have
                        && let Some(last) = span.last()
                    {
                        rows[*last] += need - have;
                    }
                }
            }
        }
        self.grids.insert(key, (columns.clone(), rows.clone()));
        (columns, rows)
    }

    /// 칸 글을 다시 쌓거나(`stack_offsets`) 줄 정보 없이 다시 배치한 표가 바깥 여백까지 저장된 줄 높이
    /// `line_height`(HWPUNIT)보다 커진 만큼. 그런 칸이 없는 표이거나 1pt 이하면 0.
    /// 커진 만큼 그 줄 뒤의 본문도 내려야 하고, 쪽에 남은 높이를 넘으면 줄째 다음 쪽으로 간다.
    pub(crate) fn stacked_growth(&mut self, table: &'a Table, line_height: f64) -> f64 {
        if is_split_table(table)
            || !table.cells.iter().any(|cell| {
                stack_offsets(cell).iter().any(|offset| *offset > 0.0)
                    || cell.paragraphs.iter().any(|para| para.line_segs.is_empty())
            })
        {
            return 0.0;
        }
        let (_, rows) = self.grid(table);
        let height = rows.iter().sum::<f64>()
            + (rows.len() + 1) as f64 * spacing(table)
            + table.out_margins.top
            + table.out_margins.bottom;
        let growth = height - line_height;
        if growth > FIT_TOLERANCE { growth } else { 0.0 }
    }

    /// 여러 행을 걸친 칸의 저장된 높이를 채우는 남는 높이(행마다). 마지막 행이 맡는다.
    /// `drawn`이 있으면 쪽에 그린 높이로, 없으면 `rows`로 잰다.
    fn merged_extra(
        &mut self,
        table: &'a Table,
        columns: &[f64],
        rows: &[f64],
        drawn: Option<&[f64]>,
    ) -> Vec<f64> {
        let spacing = spacing(table);
        let measured = drawn.unwrap_or(rows);
        let restack = !is_split_table(table);
        let mut extra = vec![0.0f64; rows.len()];
        for cell in &table.cells {
            if cell.row_span <= 1 {
                continue;
            }
            let Some(row) = index(cell.row, rows.len()) else {
                continue;
            };
            let end = rows.len().min(row + cell.row_span as usize);
            let Some(last) = end.checked_sub(1) else {
                continue;
            };
            let width = content_width(cell, columns, spacing);
            let content = self.saved_cell_content_height(cell, None, Some(width), restack)
                + cell.margins.top
                + cell.margins.bottom;
            let need = cell.height.max(content);
            // 마지막 행이 남는 높이를 맡되, 제 높이보다 작아지지는 않는다.
            let before = measured[row..last].iter().sum::<f64>() + (last - row) as f64 * spacing;
            let room = (need - before).max(0.0);
            if room > rows[last] {
                extra[last] = extra[last].max(room - rows[last]);
            }
        }
        extra
    }

    /// 칸 글의 높이(HWPUNIT): 고른 줄의 아래와, 문단을 따라 움직이는 떠 있는 개체의 아래 중 가장 큰 값.
    /// `restack`이면 겹쳐 저장된 문단을 다시 쌓은 자리로 잰다(`stack_offsets`).
    pub(crate) fn saved_cell_content_height(
        &mut self,
        cell: &'a Cell,
        lines: Option<&Lines>,
        width: Option<f64>,
        restack: bool,
    ) -> f64 {
        let width = width
            .unwrap_or(cell.width - cell.margins.left - cell.margins.right)
            .max(0.0);
        let offsets = if restack {
            stack_offsets(cell)
        } else {
            vec![0.0; cell.paragraphs.len()]
        };
        let mut bottom = 0.0f64;
        let mut next_top = 0.0f64;
        for (pi, para) in cell.paragraphs.iter().enumerate() {
            let pp = self.para_pr(para.para_pr_id);
            let offset = offsets[pi];
            if para.line_segs.is_empty() {
                if lines.is_none_or(|lines| lines.contains_key(&pi)) {
                    let segs = self.flow_segs(para, width, next_top + pp.prev);
                    if let Some(last) = segs.last() {
                        bottom = bottom.max(last.vertpos + last.vertsize);
                        next_top = last.vertpos + last.vertsize + last.spacing + pp.next;
                    }
                }
                continue;
            }
            let selected: Vec<usize> = (0..para.line_segs.len())
                .filter(|li| {
                    lines.is_none_or(|lines| lines.get(&pi).is_some_and(|set| set.contains(li)))
                })
                .collect();
            for li in &selected {
                let seg = &para.line_segs[*li];
                bottom = bottom.max(seg.vertpos + offset + seg.vertsize);
                next_top = seg.vertpos + offset + seg.vertsize + seg.spacing + pp.next;
            }
            bottom = bottom.max(self.floating_bottom(para, &selected, offset));
        }
        bottom
    }

    /// 고른 줄에 닻을 둔, 문단을 따라 움직이는 떠 있는 개체의 아래(HWPUNIT). 칸 안에서 한/글은 개체를 닻이
    /// 있는 줄의 위에서 잰다. `offset`은 문단 줄 자리에 더할 값이다.
    fn floating_bottom(&mut self, para: &'a Paragraph, selected: &[usize], offset: f64) -> f64 {
        let stream = self.atoms(para);
        let segs = &para.line_segs;
        let mut bottom = 0.0f64;
        for li in selected {
            let top = segs[*li].vertpos + offset;
            let start = (segs[*li].textpos.max(0) as usize).min(stream.len());
            let end = segs
                .get(li + 1)
                .map_or(stream.len(), |next| {
                    (next.textpos.max(0) as usize).min(stream.len())
                })
                .max(start);
            for atom in &stream[start..end] {
                let Some(item) = atom.object else {
                    continue;
                };
                let pos = super::object_pos(item);
                if pos.treat_as_char || pos.vert_rel_to != "PARA" || !pos.flow_with_text {
                    continue;
                }
                let (_, height) = self.object_size(item);
                let margins = super::object_margins(item);
                bottom = bottom.max(
                    top + pos.vert_offset.max(0.0)
                        + margins.top
                        + f64::from(height) * 100.0
                        + margins.bottom,
                );
            }
        }
        bottom
    }

    pub(crate) fn layout_table(
        &mut self,
        table: &'a Table,
        origin: (f32, f32),
        section: &'a Section,
    ) {
        let (columns, natural) = self.grid(table);
        let extra = if is_split_table(table) {
            self.merged_extra(table, &columns, &natural, None)
        } else {
            vec![0.0; natural.len()]
        };
        let rows: Vec<(usize, f64)> = natural
            .iter()
            .zip(&extra)
            .map(|(height, extra)| height + extra)
            .enumerate()
            .collect();
        self.draw_table_piece(table, &columns, &rows, &|_| None, origin, section);
    }

    /// 표의 한 조각을 `origin`(pt, 조각 왼쪽 위)에 그린다. rows: 이 조각의 (행 번호, 높이 HWPUNIT).
    /// fragment: 칸 색인 → 그릴 글 조각 번호(없으면 칸 글 전체). 칸마다 채우기 → 내용, 테두리는 모든 칸 뒤에 그린다.
    fn draw_table_piece(
        &mut self,
        table: &'a Table,
        columns: &[f64],
        rows: &[(usize, f64)],
        fragment: &dyn Fn(usize) -> Option<usize>,
        origin: (f32, f32),
        section: &'a Section,
    ) {
        let page = &section.page;
        let grid = if page_scale(self.doc.track_changes, pt(page.width), section) == 1.0 {
            LineGrid::Body(
                pt(page.margin_left + page.gutter),
                pt(page.margin_top + page.header),
            )
        } else {
            LineGrid::Scaled
        };
        let spacing = pt(spacing(table));
        let mut col_x = vec![spacing];
        for width in columns {
            col_x.push(col_x[col_x.len() - 1] + pt(*width) + spacing);
        }
        let mut row_y = vec![spacing];
        for (_, height) in rows {
            row_y.push(row_y[row_y.len() - 1] + pt(*height) + spacing);
        }
        let restack = !is_split_table(table);
        // 다음 쪽 조각 맨 위에 되풀이한 머리행 수. 머리행 뒤 행 번호가 건너뛰면 되풀이한 것이다.
        let headers = repeated_header_rows(table);
        let leading = rows
            .iter()
            .zip(&headers)
            .take_while(|((row, _), header)| row == *header)
            .count();
        let repeated = match rows.get(leading) {
            Some((row, _)) if leading > 0 && *row != leading => leading,
            _ => 0,
        };
        let mut borders: Vec<Item> = Vec::new();
        let flags = Flags {
            floating: !table.pos.treat_as_char && !self.rec.inside_object(),
            behind: self.behind,
            hidden: false,
        };
        self.rec.begin(Kind::Table, "표", "", flags);
        self.rec_extend(Rect::new(
            origin.0,
            origin.1,
            col_x[col_x.len() - 1],
            row_y[row_y.len() - 1],
        ));
        for (ci, cell) in table.cells.iter().enumerate() {
            let Some(col) = index(cell.col, columns.len()) else {
                continue;
            };
            let span = i64::from(cell.row)..i64::from(cell.row) + i64::from(cell.row_span.max(1));
            let (Some(first), Some(last)) = (
                rows.iter()
                    .position(|(row, _)| span.contains(&(*row as i64))),
                rows.iter()
                    .rposition(|(row, _)| span.contains(&(*row as i64))),
            ) else {
                continue;
            };
            let x = origin.0 + col_x[col];
            let y = origin.1 + row_y[first];
            let end = (col + cell.col_span.max(1) as usize).min(col_x.len() - 1);
            let rect = Rect::new(
                x,
                y,
                col_x[end] - col_x[col] - spacing,
                row_y[last + 1] - row_y[first] - spacing,
            );
            let span_text = if cell.row_span > 1 || cell.col_span > 1 {
                format!("{}행 {}열 병합", cell.row_span.max(1), cell.col_span.max(1))
            } else {
                String::new()
            };
            self.rec.begin(
                Kind::Cell,
                &format!("{}행 {}열", cell.row + 1, cell.col + 1),
                &span_text,
                Flags::default(),
            );
            self.rec_extend(rect);
            let fill = self.border_fills.get(&cell.border_fill_id).copied();
            if let Some(fill) = fill {
                match &fill.gradation {
                    Some(gradient) => self.gradient_fill(rect, fill.fill_color, gradient),
                    None => {
                        if let Some(color) = fill.fill_color {
                            self.fill(rect, color, 1.0);
                        }
                    }
                }
            }
            let margins = &cell.margins;
            let inner = Rect::new(
                x + pt(margins.left),
                y + pt(margins.top),
                rect.width - pt(margins.left + margins.right),
                rect.height - pt(margins.top + margins.bottom),
            );
            let only: Option<Lines> = fragment(ci).map(|k| {
                let mut map = Lines::new();
                if let Some(lines) = cell_fragments(cell).get(k) {
                    for (pi, li) in lines {
                        map.entry(*pi).or_default().insert(*li);
                    }
                }
                if k == 0 {
                    for (pi, para) in cell.paragraphs.iter().enumerate() {
                        if para.line_segs.is_empty() {
                            map.insert(pi, BTreeSet::new());
                        }
                    }
                }
                map
            });
            let content = pt(self.saved_cell_content_height(
                cell,
                only.as_ref(),
                Some(f64::from(inner.width.max(0.0)) * 100.0),
                restack,
            ));
            let offsets = if restack {
                stack_offsets(cell)
            } else {
                vec![0.0; cell.paragraphs.len()]
            };
            let dy = match cell.vert_align.as_str() {
                "CENTER" => ((inner.height - content) / 2.0).max(0.0),
                "BOTTOM" => (inner.height - content).max(0.0),
                _ => 0.0,
            };
            let mut last_vert = -1.0;
            // 한/글은 셀보다 큰 그림을 셀 테두리에서 잘라 그린다.
            self.add(Item::Clip(rect));
            for (pi, para) in cell.paragraphs.iter().enumerate() {
                let lines = match &only {
                    Some(only) => match only.get(&pi) {
                        Some(lines) => (!para.line_segs.is_empty()).then_some(lines),
                        None => continue,
                    },
                    None => None,
                };
                self.layout_paragraph(
                    para,
                    (inner.x, inner.y + dy + pt(offsets[pi])),
                    inner.width,
                    section,
                    false,
                    &mut last_vert,
                    lines,
                );
            }
            self.add(Item::Pop);
            if let Some(fill) = fill {
                let joins = border_joins(table, cell, &self.border_fills, false);
                let mut vertical_joins = border_joins(table, cell, &self.border_fills, true);
                // 칸 사이 세로 변은 왼쪽 칸의 오른 변이 정한다. 그 변이 모서리 너머로 이어지지 않으면 오른쪽
                // 칸의 왼 변도 거기서 멈춘다(한/글은 겹친 두 변 가운데 왼쪽 칸 것을 그린다).
                for (corner, joined) in vertical_joins.iter_mut().enumerate() {
                    if let Some(left) = left_neighbor(table, cell, corner == 1) {
                        joined[0] &= border_joins(table, left, &self.border_fills, true)[corner][1];
                    }
                }
                let bottom = if repeated > 0 && last + 1 == repeated {
                    Edge::Open
                } else {
                    Edge::Drawn
                };
                // 되풀이한 머리행 바로 아래 칸은 윗변을 그리지 않는다. 한/글은 그 변에 머리행의 아랫변만 그린다.
                let below_header = repeated > 0
                    && first == repeated
                    && header_bottom_drawn(
                        table,
                        &self.border_fills,
                        rows[repeated - 1].0,
                        col..end,
                    );
                // 이 조각에서 바로 윗행을 함께 그릴 때만 그 행의 세로 변을 본다.
                let above = match first.checked_sub(1).map(|prev| rows[prev].0) {
                    Some(prev) if prev + 1 == rows[first].0 => [col, end]
                        .map(|boundary| vertical_at(table, &self.border_fills, prev, boundary)),
                    _ => [None; 2],
                };
                frame_items(
                    fill,
                    rect,
                    grid,
                    (
                        if below_header {
                            Edge::Hidden
                        } else {
                            Edge::Drawn
                        },
                        bottom,
                    ),
                    joins,
                    vertical_joins,
                    above,
                    [0, 1].map(|row| {
                        [0, 1].map(|col| crossing_double(table, &self.border_fills, cell, row, col))
                    }),
                    &mut borders,
                );
                if fill.slash != "NONE" {
                    border_items(
                        &fill.diagonal,
                        (rect.x, rect.bottom()),
                        (rect.right(), rect.y),
                        &mut borders,
                    );
                }
                if fill.back_slash != "NONE" {
                    border_items(
                        &fill.diagonal,
                        (rect.x, rect.y),
                        (rect.right(), rect.bottom()),
                        &mut borders,
                    );
                }
            }
            self.rec.end();
        }
        // 한/글은 셀이 맞붙은 표에서는 셀 테두리만 그리고, 표 테두리는 셀 간격이 있을 때 바깥 틀로만 그린다.
        if table.cell_spacing > 0.0
            && let Some(fill) = self.border_fills.get(&table.border_fill_id).copied()
            && let (Some(first), Some(last)) = (rows.first(), rows.last())
        {
            let rect = Rect::new(
                origin.0,
                origin.1,
                col_x[col_x.len() - 1],
                row_y[row_y.len() - 1],
            );
            let last_row = table.rows.max(1) as usize - 1;
            let edge = |drawn: bool| if drawn { Edge::Drawn } else { Edge::Hidden };
            let drawn = (edge(first.0 == 0), edge(last.0 == last_row));
            frame_items(
                fill,
                rect,
                grid,
                drawn,
                [[false; 2]; 2],
                [[false; 2]; 2],
                [None; 2],
                [[None; 2]; 2],
                &mut borders,
            );
        }
        for item in borders {
            self.add(item);
        }
        self.rec.end();
    }

    /// 나뉜 표의 조각별 (행 번호, 그릴 높이 HWPUNIT). 첫 조각은 저장된 표 높이까지, 다음 조각은 `capacity`까지 채운다.
    /// 쪽 끝에 걸린 행은 남은 자리까지 늘여 그리고, 내용이 남으면 다음 조각으로 넘긴다.
    fn split_pieces(
        &mut self,
        table: &'a Table,
        capacity: f64,
        first_capacity: f64,
    ) -> (Vec<Vec<(usize, f64)>>, bool) {
        let saved = is_split_table(table);
        let (columns, natural) = self.grid(table);
        let stored = stored_row_heights(table);
        let spacing = spacing(table);
        let count = natural.len();
        let mut fragment_count = vec![1usize; count];
        let mut fragment_heights: Vec<Vec<f64>> = vec![Vec::new(); count];
        for cell in &table.cells {
            if cell.row_span != 1 {
                continue;
            }
            let Some(row) = index(cell.row, count) else {
                continue;
            };
            let fragments = cell_fragments(cell);
            fragment_count[row] = fragment_count[row].max(fragments.len());
            for (k, fragment) in fragments.iter().enumerate() {
                let height = fragment
                    .iter()
                    .map(|(pi, li)| {
                        let seg = &cell.paragraphs[*pi].line_segs[*li];
                        seg.vertpos + seg.vertsize
                    })
                    .fold(0.0, f64::max)
                    + cell.margins.top
                    + cell.margins.bottom;
                let list = &mut fragment_heights[row];
                if list.len() <= k {
                    list.resize(k + 1, 0.0);
                }
                list[k] = list[k].max(height);
            }
        }
        // 조각이 없는 행의 내용 높이: 쪽 끝에 남은 자리에 내용이 들어가는지 볼 때 쓴다.
        let mut content = vec![0.0f64; count];
        for cell in &table.cells {
            if cell.row_span != 1 {
                continue;
            }
            let Some(row) = index(cell.row, count) else {
                continue;
            };
            if fragment_count[row] > 1 {
                continue;
            }
            let width = content_width(cell, &columns, spacing);
            let height = self.saved_cell_content_height(cell, None, Some(width), !saved)
                + cell.margins.top
                + cell.margins.bottom;
            content[row] = content[row].max(height);
        }
        // 행 가운데서 잘릴 때 쪽 끝에 비우는 높이: 칸의 아래 안 여백 − 아래 선 굵기(HWPUNIT).
        let mut cut_reserve = vec![0.0f64; count];
        for cell in &table.cells {
            if cell.row_span != 1 {
                continue;
            }
            let Some(row) = index(cell.row, count) else {
                continue;
            };
            let line = self
                .border_fills
                .get(&cell.border_fill_id)
                .filter(|fill| fill.bottom.border_type != "NONE")
                .map_or(0.0, |fill| fill.bottom.width * 7200.0 / 25.4);
            cut_reserve[row] = cut_reserve[row].max(cell.margins.bottom - line);
        }
        let header_rows = repeated_header_rows(table);
        let mut stretched = false;
        let mut build = |heights: &[f64]| -> Vec<Vec<(usize, f64)>> {
            let mut pieces: Vec<Vec<(usize, f64)>> = vec![Vec::new()];
            let mut used = spacing;
            let mut cap = if saved && table.height > 0.0 {
                table.height
            } else {
                first_capacity
            } - spacing;
            let mut body_rows = 0;
            let new_piece = |pieces: &mut Vec<Vec<(usize, f64)>>,
                             used: &mut f64,
                             cap: &mut f64,
                             body_rows: &mut usize| {
                let mut piece = Vec::new();
                *used = spacing;
                *cap = capacity - spacing;
                *body_rows = 0;
                for row in &header_rows {
                    piece.push((*row, heights[*row]));
                    *used += heights[*row] + spacing;
                }
                pieces.push(piece);
            };
            for row in 0..count {
                let row_stored = if stored[row] > 0.0 {
                    stored[row]
                } else {
                    heights[row]
                };
                if fragment_count[row] > 1 {
                    let mut done = 0.0;
                    for k in 0..fragment_count[row] {
                        let height = fragment_heights[row].get(k).copied().unwrap_or(0.0);
                        if k + 1 < fragment_count[row] {
                            let part = (cap - used - spacing).max(height);
                            if let Some(piece) = pieces.last_mut() {
                                piece.push((row, part));
                            }
                            done += part;
                            new_piece(&mut pieces, &mut used, &mut cap, &mut body_rows);
                        } else {
                            let part = height.max(row_stored - done);
                            if let Some(piece) = pieces.last_mut() {
                                piece.push((row, part));
                            }
                            used += part + spacing;
                            body_rows += 1;
                        }
                    }
                } else if used + heights[row] + spacing <= cap + FIT_TOLERANCE {
                    if let Some(piece) = pieces.last_mut() {
                        piece.push((row, heights[row]));
                    }
                    used += heights[row] + spacing;
                    body_rows += 1;
                } else if saved
                    && (body_rows > 0 || pieces.len() == 1)
                    && content[row] < heights[row]
                    && used + content[row] + spacing <= cap + FIT_TOLERANCE
                {
                    // 내용은 남은 자리에 들어간다: 한/글은 행을 쪽 아래 끝까지 늘여 그린다. 첫 조각의 첫 행이면
                    // 저장된 표 높이(첫 조각 높이)까지 줄여 그린다.
                    // 다음 조각에서는 잘린 칸의 아래 안 여백에서 아래 선 굵기를 뺀 만큼 쪽 끝을 비운다.
                    let reserve = if pieces.len() > 1 {
                        cut_reserve[row]
                    } else {
                        0.0
                    };
                    let part = (cap - used - spacing - reserve).max(0.0);
                    if let Some(piece) = pieces.last_mut() {
                        piece.push((row, part));
                    }
                    stretched = true;
                    if row + 1 < count {
                        new_piece(&mut pieces, &mut used, &mut cap, &mut body_rows);
                    }
                } else {
                    if body_rows > 0 || (!saved && pieces.len() == 1 && cap < capacity - spacing) {
                        // 걸친 칸 가운데서 나뉘면 한/글은 쪽의 마지막 행을 쪽 아래 끝까지 늘여 그린다.
                        if spans_across(table, row)
                            && let Some((_, height)) =
                                pieces.last_mut().and_then(|piece| piece.last_mut())
                        {
                            *height += (cap - used).max(0.0);
                        }
                        new_piece(&mut pieces, &mut used, &mut cap, &mut body_rows);
                    }
                    if let Some(piece) = pieces.last_mut() {
                        piece.push((row, heights[row]));
                    }
                    used += heights[row] + spacing;
                    body_rows += 1;
                }
            }
            pieces.retain(|piece| !piece.is_empty());
            pieces
        };
        // 걸친 칸의 남는 높이는 그 행이 쪽에 실제로 그린 높이로 정한다. 조각 높이가 바뀌면 다시 짓는다.
        let mut heights = natural.clone();
        let mut pieces = build(&heights);
        for _ in 0..SETTLE_PASSES {
            let mut drawn = vec![0.0f64; count];
            for piece in &pieces {
                for (row, height) in piece {
                    drawn[*row] += *height;
                }
            }
            let extra = self.merged_extra(table, &columns, &natural, Some(&drawn));
            let mut changed = false;
            for (index, extra) in extra.iter().enumerate() {
                let height = natural[index] + extra;
                if (height - heights[index]).abs() > 0.5 {
                    changed = true;
                }
                heights[index] = height;
            }
            if !changed {
                break;
            }
            pieces = build(&heights);
        }
        (pieces, stretched)
    }

    /// 쪽에서 나뉜 표를 조각마다 제 단·쪽에 그린다. 첫 조각은 `rect`의 왼쪽 위에서, 다음 조각은 다음 단(없으면 다음 쪽)
    /// 본문 위 + 바깥 위 여백에서 시작한다. 돌려주는 값은 마지막 조각의 아래 + 바깥 아래 여백(HWPUNIT, 본문 위 기준).
    /// 나뉜 표가 아니거나 조각이 하나면 아무것도 그리지 않고 None.
    pub(crate) fn place_split_table(
        &mut self,
        table: &'a Table,
        rect: Rect,
        origin: (f32, f32),
        section: &'a Section,
        reflow: bool,
    ) -> Option<f64> {
        if table.page_break_mode == "NONE" || table.rows <= 0 {
            return None;
        }
        let page = &section.page;
        let body_height =
            page.height - page.margin_top - page.header - page.margin_bottom - page.footer;
        let capacity = body_height - table.out_margins.top - table.out_margins.bottom;
        let first_capacity = ((f64::from(origin.1 + pt(body_height) - rect.y) * 100.0)
            - table.out_margins.bottom)
            .clamp(0.0, capacity.max(0.0));
        if !is_split_table(table) {
            // 줄 정보 없는 칸 글은 한/글이 다시 배치한다. 그래서 늘어난 표는 쪽에서 나뉜다.
            // 글자처럼 취급한 표는 줄 하나라 나누지 않는다. 다만 문단까지 줄 정보가 없으면 예전처럼 나눈다.
            let paras = || table.cells.iter().flat_map(|cell| &cell.paragraphs);
            let reflowed = if table.pos.treat_as_char {
                reflow && paras().all(|para| para.line_segs.is_empty())
            } else {
                paras().any(|para| para.line_segs.is_empty())
            };
            if !reflowed {
                return None;
            }
            let (_, rows) = self.grid(table);
            if rows.iter().sum::<f64>() + f64::from(table.rows + 1) * spacing(table)
                <= first_capacity + FIT_TOLERANCE
            {
                return None;
            }
        }
        let (pieces, stretched) = self.split_pieces(table, capacity, first_capacity);
        if pieces.len() < 2 && !stretched {
            return None;
        }
        let (columns, _) = self.grid(table);
        let header_rows: HashSet<usize> = repeated_header_rows(table).into_iter().collect();
        let mut seen = vec![0usize; table.cells.len()];
        let (mut x, mut y) = (rect.x, rect.y);
        let mut bottom = 0.0;
        for (i, piece) in pieces.iter().enumerate() {
            if i > 0 {
                let old_column = self.column;
                if self.column + 1 < self.column_count {
                    self.column += 1;
                } else {
                    let index = self.page.map_or(0, |page| self.pages[page].section);
                    self.new_page(section, index);
                    self.column = 0;
                }
                self.object_bottom = 0.0;
                self.object_top = 0.0;
                self.stack_shift = 0.0;
                self.stacked_bottom = None;
                x += (self.column as f32 - old_column as f32) * self.column_step;
                y = origin.1 + pt(table.out_margins.top);
            }
            let repeated: HashSet<usize> = if i > 0 {
                piece
                    .iter()
                    .take_while(|(row, _)| header_rows.contains(row))
                    .map(|(row, _)| *row)
                    .collect()
            } else {
                HashSet::new()
            };
            let mut fragments: HashMap<usize, Option<usize>> = HashMap::new();
            for (ci, cell) in table.cells.iter().enumerate() {
                let span =
                    i64::from(cell.row)..i64::from(cell.row) + i64::from(cell.row_span.max(1));
                if !piece.iter().any(|(row, _)| span.contains(&(*row as i64))) {
                    continue;
                }
                if usize::try_from(cell.row).is_ok_and(|row| repeated.contains(&row)) {
                    fragments.insert(ci, None);
                    continue;
                }
                fragments.insert(ci, Some(seen[ci]));
                seen[ci] += 1;
            }
            let lookup = |ci: usize| fragments.get(&ci).copied().flatten();
            self.draw_table_piece(table, &columns, piece, &lookup, (x, y), section);
            let height = piece.iter().map(|(_, height)| height).sum::<f64>()
                + (piece.len() + 1) as f64 * spacing(table);
            self.extend_columns(y, y + pt(height));
            bottom = table.out_margins.top + height + table.out_margins.bottom;
        }
        Some(bottom)
    }
}
