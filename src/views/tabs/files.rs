#![allow(dead_code)]

use deluge_rpc::{FilePriority, Query, Session, InfoHash};
use serde::Deserialize;
use slab::Slab;
use std::collections::HashMap;
use std::cmp::Ordering;
use cursive::Printer;
use crate::util;
use std::sync::{Arc, RwLock};
use super::TabData;
use async_trait::async_trait;
use cursive::view::ScrollBase;
use cursive::Vec2;
use cursive::View;
use tokio::time;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column { Filename, Size, Progress, Priority }
impl AsRef<str> for Column {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::Filename => "Filename",
            Self::Size => "Size",
            Self::Progress => "Progress",
            Self::Priority => "Priority",
        }
    }
}
impl Default for Column { fn default() -> Self { Self::Filename } }

struct File {
    parent: usize,
    index: usize,
    depth: usize,
    name: String,
    size: u64,
    progress: f64,
    priority: FilePriority,
}

#[derive(Default)]
struct Dir {
    parent: Option<usize>,
    depth: usize,
    name: String,
    children: HashMap<String, DirEntry>,
    descendants: Vec<usize>,
    size: u64,
    progress: f64,
    collapsed: bool,
}

#[derive(Debug, Clone, Copy)]
enum DirEntry {
    File(usize), // an index into a Vec<File>
    Dir(usize),  // an index into a Slab<Dir>
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct QueryFile {
    index: usize,
    offset: u64,
    path: String,
    size: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Query)]
struct FilesQuery {
    files: Vec<QueryFile>,
    file_progress: Vec<f64>,
    file_priorities: Vec<FilePriority>,
}

#[derive(Default)]
struct FilesState {
    rows: Vec<DirEntry>,
    files_info: Vec<File>,
    // TODO: write a simpler Slab with more applicable invariants
    // Would also be usable for files_info.
    dirs_info: Slab<Dir>,
    root_dir: usize,
    sort_column: Column,
    descending_sort: bool,
}

impl FilesState {
    fn get_size(&self, entry: DirEntry) -> u64 {
        match entry {
            DirEntry::Dir(id) => self.dirs_info[id].size,
            DirEntry::File(id) => self.files_info[id].size,
        }
    }

    fn get_progress(&self, entry: DirEntry) -> f64 {
        match entry {
            DirEntry::Dir(id) => self.dirs_info[id].progress,
            DirEntry::File(id) => self.files_info[id].progress,
        }
    }

    fn get_depth(&self, entry: DirEntry) -> usize {
        match entry {
            DirEntry::Dir(id) => self.dirs_info[id].depth,
            DirEntry::File(id) => self.files_info[id].depth,
        }
    }

    fn get_parent(&self, entry: DirEntry) -> Option<usize> {
        match entry {
            DirEntry::Dir(id) => self.dirs_info[id].parent,
            DirEntry::File(id) => Some(self.files_info[id].parent),
        }
    }

    fn is_parent(&self, possible_parent: usize, possible_child: DirEntry) -> bool {
        if self.get_depth(possible_child) <= self.dirs_info[possible_parent].depth {
            return false;
        }

        let mut parent_id = self.get_parent(possible_child);

        // Recursion avoided for the sake of avoiding recursion.
        while let Some(id) = parent_id {
            if id == possible_parent {
                return true;
            }
            parent_id = self.dirs_info[id].parent;
        }

        false
    }

    fn compare_dir_entries(&self, a: DirEntry, b: DirEntry) -> Ordering {
        // "Weak" invariant
        assert_eq!(self.get_parent(a), self.get_parent(b), "Non-sibling dir entries must not be compared");
        // "Strong" invariant
        assert_eq!(self.get_depth(a), self.get_depth(b), "Sibling dir entries must have the same depth");

        match (a, b) {
            (DirEntry::Dir(_), DirEntry::File(_)) => Ordering::Greater,
            (DirEntry::File(_), DirEntry::Dir(_)) => Ordering::Less,

            (DirEntry::Dir(a), DirEntry::Dir(b)) => {
                let (a, b) = (&self.dirs_info[a], &self.dirs_info[b]);

                match self.sort_column {
                    Column::Filename => a.name.cmp(&b.name).reverse(),
                    Column::Size => a.size.cmp(&b.size),
                    Column::Progress => a.progress.partial_cmp(&b.progress).expect("well-behaved floats"),
                    Column::Priority => Ordering::Equal,
                }
            },

            (DirEntry::File(a), DirEntry::File(b)) => {
                let (a, b) = (&self.files_info[a], &self.files_info[b]);

                match self.sort_column {
                    Column::Filename => a.name.cmp(&b.name).reverse(),
                    Column::Size => a.size.cmp(&b.size),
                    Column::Progress => a.progress.partial_cmp(&b.progress).expect("well-behaved floats"),
                    Column::Priority => a.priority.cmp(&b.priority),
                }
            }
        }
    }

    fn build_tree(&mut self, query: FilesQuery) {
        let FilesQuery { files, file_progress, file_priorities } = query;

        assert_eq!(files.len(), file_progress.len());
        assert_eq!(files.len(), file_priorities.len());

        self.files_info.clear();
        self.files_info.reserve_exact(files.len());
        self.dirs_info.clear();
        self.dirs_info.reserve_exact(files.len()); // hey, it's an upper bound

        self.root_dir = self.dirs_info.insert(Dir::default());

        for (i, file) in files.into_iter().enumerate() {
            let mut cwd = self.root_dir;

            assert_eq!(i, file.index);
            let progress = file_progress[i];
            let priority = file_priorities[i];

            let mut depth = self.dirs_info[cwd].depth;
            assert_eq!(depth, 0);

            let (dir_names, file_name) = {
                let mut iter = file.path.split('/');
                let last = iter.next_back().unwrap();
                // TODO: Result
                assert!(!last.is_empty());
                (iter, last)
            };

            for dir_name in dir_names {
                // TODO: Result
                assert!(!dir_name.is_empty());
                depth += 1;
                self.dirs_info[cwd].descendants.push(i);

                if let Some(entry) = self.dirs_info[cwd].children.get(dir_name) {
                    cwd = match entry {
                        DirEntry::Dir(id) => {
                            assert_eq!(depth, self.dirs_info[*id].depth);
                            *id
                        },
                        // TODO: Result
                        DirEntry::File(_) => panic!("Unexpected file"),
                    };
                } else {
                    let d = Dir {
                        parent: Some(cwd),
                        depth,
                        name: String::from(dir_name),
                        ..Dir::default()
                    };
                    let dir_name = d.name.clone();
                    let child_id = self.dirs_info.insert(d);

                    self.dirs_info[cwd]
                        .children
                        .insert(dir_name, DirEntry::Dir(child_id));

                    cwd = child_id;
                }
            }

            depth += 1;

            let f = File {
                parent: cwd,
                index: file.index,
                size: file.size,
                name: String::from(file_name),
                depth,
                progress,
                priority,
            };

            assert_eq!(self.files_info.len(), i);
            self.files_info.push(f);
            let file_name = &self.files_info[i].name;

            debug_assert!(!self.dirs_info[cwd].descendants.contains(&i));
            self.dirs_info[cwd].descendants.push(i);

            // TODO: Result
            assert!(!self.dirs_info[cwd].children.contains_key(file_name));
            self.dirs_info[cwd]
                .children
                .insert(file_name.clone(), DirEntry::File(i));
        }

        self.files_info.shrink_to_fit();
        self.dirs_info.shrink_to_fit();

        self.update_dir_values();
    }

    fn update_dir_values_owned(self) -> Self {
        let mut dirs_info = self.dirs_info;
        let files_info = &self.files_info;

        for (_, dir) in dirs_info.iter_mut() {
            dir.size = 0;
            dir.progress = 0.0;

            let files = dir.descendants.iter().map(|id| &files_info[*id]);

            for file in files {
                dir.size += file.size;
                dir.progress += file.progress;
            }

            dir.progress /= dir.descendants.len() as f64;
        }

        Self { dirs_info, ..self }
    }

    fn update_dir_values(&mut self) {
        take_mut::take_or_recover(self, Self::default, Self::update_dir_values_owned);
    }

    fn push_entry(&self, rows: &mut Vec<DirEntry>, entry: DirEntry) {
        rows.push(entry);

        if let DirEntry::Dir(id) = entry {
            if !self.dirs_info[id].collapsed {
                // TODO: find a way to do this before building the rows
                // or something
                // I don't really know
                // this is a really complicated problem
                let mut children: Vec<DirEntry> = self.dirs_info[id]
                    .children
                    .values()
                    .copied()
                    .collect();

                children.sort_unstable_by(|&a, &b| self.compare_dir_entries(a, b));

                // welcome to recursion land
                for child in children.into_iter() {
                    self.push_entry(rows, child);
                }
            }
        }
    }

    fn rebuild_rows(&mut self) {
        let mut rows = std::mem::replace(&mut self.rows, Vec::new());
        rows.clear();
        self.push_entry(&mut rows, DirEntry::Dir(self.root_dir));
        self.rows = rows;
    }

    fn draw_cell(&self, printer: &Printer, entry: DirEntry, col: Column) {
        match (col, entry) {
            (Column::Filename, DirEntry::Dir(id)) => {
                let dir = &self.dirs_info[id];
                let c = if dir.collapsed { '▸' } else { '▾' };
                let text = format!("{} {}", c, dir.name);
                printer.print((dir.depth, 0), &text);
            },

            (Column::Filename, DirEntry::File(id)) => {
                let file = &self.files_info[id];
                printer.print((file.depth, 0), &file.name);
            },

            (Column::Size, entry) => {
                let size = self.get_size(entry);
                printer.print((0, 0), &util::fmt_bytes(size));
            },

            (Column::Progress, entry) => {
                let progress = self.get_progress(entry);
                printer.print((0, 0), &progress.to_string());
            },

            (Column::Priority, DirEntry::Dir(_)) => (),

            (Column::Priority, DirEntry::File(id)) => {
                let priority = self.files_info[id].priority;
                // TODO: this is missing from deluge_rpc
                let s = match priority {
                    FilePriority::Skip => "Skip",
                    FilePriority::Low => "Low",
                    FilePriority::Normal => "Normal",
                    FilePriority::High => "High",
                };
                printer.print((0, 0), s);
            },
        }
    }

    fn draw_row(&self, columns: &[(Column, usize)], printer: &Printer, entry: DirEntry) {
        let mut x = 0;
        for (column, width) in columns {
            self.draw_cell(&printer.offset((x, 0)).cropped((*width, 1)), entry, *column);
            x += width + 1;
        }
    }

    /*
    fn click_column(&mut self, column: Column) {
        if column == self.sort_column {
            self.descending_sort = !self.descending_sort;
        } else {
            self.sort_column = column;
            self.descending_sort = true;
        }
        // TODO: would figuring out how to do stuff in-place be meaningfully cheaper?
        self.rebuild_rows();
    }
    */
}

pub(super) struct FilesView {
    state: Arc<RwLock<FilesState>>,
    columns: Vec<(Column, usize)>,
    scrollbase: ScrollBase,
}

impl View for FilesView {
    // TODO: figure out what this, the torrents tab, and the files tab have in common.
    // Move that to shared code.
    fn draw(&self, printer: &Printer) {
        let Vec2 { x: w, y: h } = printer.size;

        let data = self.state.read().unwrap();

        let mut x = 0;
        for (column, width) in &self.columns {
            let mut name = String::from(column.as_ref());

            if *column == data.sort_column {
                name.push_str(if data.descending_sort { " v" } else { " ^" });
            }

            printer.cropped((x+width, 1)).print((x, 0), &name);
            printer.print_hline((x, 1), *width, "─");
            x += width;
            if x == w - 1 {
                printer.print((x, 1), "─");
                break;
            }
            printer.print_vline((x, 0), h, "│");
            printer.print((x, 1), "┼");
            x += 1;
        }
        printer.print((0, 1), "╶");

        self.scrollbase.draw(&printer.offset((0, 2)), |p, i| {
            if let Some(entry) = data.rows.get(i) {
                p.with_selection(
                    false, // TODO: clicking on rows to do stuff
                    |p| data.draw_row(&self.columns, p, *entry),
                );
            }
        });
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        constraint
    }

    fn layout(&mut self, size: Vec2) {
        self.columns[0].1 = size.x - 34;
        self.columns[1].1 = 10;
        self.columns[2].1 = 10;
        self.columns[3].1 = 10;
        let sb = &mut self.scrollbase;
        sb.view_height = size.y - 2;
        sb.content_height = self.state.read().unwrap().rows.len();
        sb.start_line = sb.start_line.min(sb.content_height.saturating_sub(sb.view_height));
    }

    fn take_focus(&mut self, _: cursive::direction::Direction) -> bool { true }
}

#[derive(Default)]
pub(super) struct FilesData {
    state: Arc<RwLock<FilesState>>,
    active_torrent: Option<InfoHash>,
}

#[async_trait]
impl TabData for FilesData {
    type V = FilesView;

    fn view() -> (Self::V, Self) {
        let state = Arc::new(RwLock::new(FilesState::default()));
        let view = {
            FilesView {
                state: state.clone(),
                scrollbase: ScrollBase::default(),
                columns: vec![
                    (Column::Filename, 10),
                    (Column::Size, 10),
                    (Column::Progress, 10),
                    (Column::Priority, 10),
                ],
            }
        };
        let data = FilesData { state, active_torrent: None };
        (view, data)
    }

    async fn update(&mut self, session: &Session, hash: InfoHash) -> deluge_rpc::Result<()> {
        let deadline = time::Instant::now() + time::Duration::from_secs(1);
        self.active_torrent = Some(hash);

        let query = session.get_torrent_status::<FilesQuery>(hash).await?;

        {
            let mut state = self.state.write().unwrap();
            state.build_tree(query);
            state.rebuild_rows();
        }

        time::delay_until(deadline).await;

        Ok(())
    }
}
