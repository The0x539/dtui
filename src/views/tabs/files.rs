#![allow(dead_code)]

use deluge_rpc::{FilePriority, Query};
use serde::Deserialize;
use slab::Slab;
use std::collections::HashMap;
//use std::cmp::Ordering;

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

struct FilesData {
    rows: Vec<DirEntry>,
    files_info: Vec<File>,
    dirs_info: Slab<Dir>,
    root_dir: usize,
    sort_column: Column,
    descending_sort: bool,
}

impl FilesData {
    /*
    fn get_name(&self, entry: DirEntry) -> &str {
        match entry {
            DirEntry::Dir(id) => &self.dirs_info[id].name,
            DirEntry::File(id) => &self.files_info[id].name,
        }
    }
    */

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

    /*
    fn compare_dir_entries(&self, a: DirEntry, b: DirEntry) -> Ordering {
        match (a, b) {
            (DirEntry::Dir(_), DirEntry::File(_)) => Ordering::Greater,
            (DirEntry::File(_), DirEntry::Dir(_)) => Ordering::Less,

            (DirEntry::Dir(a), DirEntry::Dir(b)) => {
                let (a, b) = (&self.dirs_info[a], &self.dirs_info[b]);

                match self.sort_column {
                    Column::Filename => a.name.cmp(&b.name).reverse(),
                    _ => todo!(),
                }
            },

            (DirEntry::File(a), DirEntry::File(b)) => {
                let (a, b) = (&self.files_info[a], &self.files_info[b]);

                match self.sort_column {
                    Column::Filename => a.name.cmp(&b.name).reverse(),
                    _ => todo!(),
                }
            }
        }
    }
    */

    fn build_tree(&mut self, query: FilesQuery) {
        let FilesQuery { files, file_progress, file_priorities } = query;

        assert_eq!(files.len(), file_progress.len());
        assert_eq!(files.len(), file_priorities.len());

        self.files_info.clear();
        self.files_info.reserve_exact(files.len());
        self.dirs_info.clear();

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
                        DirEntry::Dir(id) => *id,
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

            // TODO: Result
            assert!(!self.dirs_info[cwd].children.contains_key(file_name));
            self.dirs_info[cwd]
                .children
                .insert(file_name.clone(), DirEntry::File(i));
        }
    }
}
