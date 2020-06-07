#![allow(dead_code)]

use deluge_rpc::{FilePriority, Query};
use serde::Deserialize;
use slab::Slab;

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
    name: String,
    depth: usize,
    size: u64,
    progress: f64,
    priority: FilePriority,
}

#[derive(Default)]
struct Dir {
    parent: Option<usize>,
    name: String,
    depth: usize,
    children: Vec<DirEntry>,
    descendants: Vec<usize>,
}

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

fn build_tree(query: FilesQuery, files_info: &mut Vec<File>, dirs_info: &mut Slab<Dir>) -> usize {
    let FilesQuery { files, file_progress, file_priorities } = query;

    assert_eq!(files.len(), file_progress.len());
    assert_eq!(files.len(), file_priorities.len());

    files_info.clear();
    files_info.reserve_exact(files.len());
    dirs_info.clear();

    let root = dirs_info.insert(Dir::default());

    for (i, file) in files.into_iter().enumerate() {
        let mut cwd = root;
        dirs_info[cwd].descendants.push(i);

        assert_eq!(i, file.index);
        let progress = file_progress[i];
        let priority = file_priorities[i];

        let mut iter = file.path.split('/').peekable();

        while let Some(segment) = iter.next() {
            let segment = String::from(segment);

            let depth = dirs_info[cwd].depth + 1;

            if iter.peek().is_none() {
                let f = File {
                    parent: cwd,
                    index: file.index,
                    size: file.size,
                    name: segment,
                    depth,
                    progress,
                    priority,
                };

                assert_eq!(files_info.len(), i);
                files_info.push(f);
                dirs_info[cwd].children.push(DirEntry::File(i));

                break;
            } else {
                let d = Dir {
                    parent: Some(cwd),
                    name: segment,
                    depth,
                    ..Dir::default()
                };

                let child_key = dirs_info.insert(d);
                dirs_info[cwd].children.push(DirEntry::Dir(child_key));
                cwd = child_key;
            }
        }
    }

    root
}

/*
#[derive(Debug, Default, Clone)]
struct FilesData {
    rows: Vec<Row>,
    files: Directory,
    sort_column: Column,
    descending_sort: bool,
}

impl FilesData {
    fn compare_rows(&self, a: &Row, b: &Row) -> Ordering {
        match (a, b) {
            (Row::Directory(_), Row::File(_)) => Ordering::Greater,
            (Row::File(_), Row::Directory(_)) => Ordering::Less,
            (Row::Directory(_, name, size, progress), Row::Directory(_, name, size, progress)) => {
                match self.sort_column {
                    Column::Filename => 
                }
            },
            (Row::File(name, size, progress, priority), Row::File(name, size, progress, priority)) => {

            }
        }
    }
}
*/
