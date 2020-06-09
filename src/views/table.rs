use std::cmp::Ordering;
use std::ops::DerefMut;
use std::sync::{Arc, RwLock};

use cursive::Printer;
use cursive::View;
use cursive::view::ScrollBase;
use cursive::Vec2;
use cursive::event::{Event, EventResult, MouseEvent, MouseButton};
use cursive::direction::Direction;

pub(crate) trait TableViewData: Default {
    type Column: Copy + Eq + AsRef<str>;
    type Row: Copy + Eq;
    type Rows: DerefMut<Target = [Self::Row]> + Default;

    fn sort_column(&self) -> Self::Column;
    fn set_sort_column(&mut self, val: Self::Column);

    fn descending_sort(&self) -> bool;
    fn set_descending_sort(&mut self, val: bool);

    fn reverse_rows(&mut self) {
        self.set_descending_sort(!self.descending_sort());
    }

    fn rows(&self) -> &Self::Rows;
    fn rows_mut(&mut self) -> &mut Self::Rows;
    fn set_rows(&mut self, val: Self::Rows);

    fn compare_rows(&self, a: &Self::Row, b: &Self::Row) -> Ordering;

    fn sort_unstable(&mut self) {
        let mut rows = std::mem::replace(self.rows_mut(), Self::Rows::default());
        rows.sort_unstable_by(|a, b| self.compare_rows(a, b));
        self.set_rows(rows);
    }

    fn sort_stable(&mut self) {
        let mut rows = std::mem::replace(self.rows_mut(), Self::Rows::default());
        rows.sort_by(|a, b| self.compare_rows(a, b));
        self.set_rows(rows);
    }

    fn click_column(&mut self, column: Self::Column) {
        if column == self.sort_column() {
            self.reverse_rows();
        } else {
            self.set_sort_column(column);
        }
    }

    fn draw_cell(&self, printer: &Printer, row: &Self::Row, column: Self::Column);

    fn draw_row(
        &self,
        printer: &Printer,
        columns: &[(Self::Column, usize)],
        row: &Self::Row,
    ) {
        let mut x = 0;
        for (column, width) in columns {
            let printer = printer.offset((x, 0)).cropped((*width, 1));
            self.draw_cell(&printer, row, *column);
            x += width + 1;
        }
    }
}

macro_rules! impl_table {
    (
        sort_column: $col_ty:ty = self.$col:ident;
        rows: Vec<$row_ty:ty> = self.$rows:ident; // this is all I need; sue me
        descending_sort = self.$sort:ident;
    ) => {
        type Column = $col_ty;
        type Row = $row_ty;
        type Rows = Vec<Self::Row>;

        fn sort_column(&self) -> Self::Column { self.$col }
        fn descending_sort(&self) -> bool { self.$sort }
        fn rows(&self) -> &Self::Rows { &self.$rows }
        fn rows_mut(&mut self) -> &mut Self::Rows { &mut self.$rows }
        fn set_rows(&mut self, val: Self::Rows) { self.$rows = val; }
    }
}

pub(crate) struct TableView<T: TableViewData> {
    pub data: Arc<RwLock<T>>,
    pub columns: Vec<(T::Column, usize)>,
    scrollbase: ScrollBase,
    pub selected: Option<T::Row>,
}

impl<T: TableViewData> TableView<T> {
    pub fn new(columns: Vec<(T::Column, usize)>) -> Self {
        Self {
            data: Arc::new(RwLock::new(T::default())),
            columns,
            scrollbase: ScrollBase::default(),
            selected: None,
        }
    }

    fn click_header(&mut self, mut x: usize) {
        for (column, width) in &self.columns {
            if x < *width {
                self.data.write().unwrap().click_column(*column);
                return;
            } else if x == *width {
                // a column separator was clicked; do nothing
                return;
            }
            x -= width + 1;
        }
    }

    fn width(&self) -> usize {
        self.columns
            .iter()
            .map(|(_, w)| w + 1)
            .sum::<usize>()
            //.saturating_sub(1)
    }
}

impl<T: TableViewData> View for TableView<T> where Self: 'static {
    fn draw(&self, printer: &Printer) {
        let Vec2 { x: w, y: h } = printer.size;

        let data = self.data.read().unwrap();

        let mut x = 0;
        for (column, width) in &self.columns {
            let mut name = String::from(column.as_ref());

            if *column == data.sort_column() {
                let c = if data.descending_sort() { " v" } else { " ^" };
                name.push_str(c);
            }

            printer.cropped((x + width, 1)).print((x, 0), &name);
            printer.print_hline((x, 1), *width, "─");
            x += width;
            if x == w - 1 {
                printer.print((x, 1), "X");
                break;
            }
            printer.print_vline((x, 0), h, "│");
            printer.print((x, 1), "┼");
            x += 1;
        }
        printer.print((0, 1), "╶");

        self.scrollbase.draw(&printer.offset((0, 2)), |p, i| {
            if let Some(row) = data.rows().get(i) {
                p.with_selection(
                    self.selected.contains(row),
                    |p| data.draw_row(p, &self.columns, row),
                );
            }
        });
    }

    fn required_size(&mut self, req: Vec2) -> Vec2 {
        req
    }

    fn layout(&mut self, size: Vec2) {
        let others_width = self.columns[1..]
            .iter()
            .map(|(_, w)| w+1)
            .sum::<usize>();

        self.columns[0].1 = size.x - others_width;

        let sb = &mut self.scrollbase;

        sb.view_height = size.y - 2;
        sb.content_height = self.data.read().unwrap().rows().len();
        sb.start_line = sb.start_line.min(sb.content_height.saturating_sub(sb.view_height));
    }

    fn take_focus(&mut self, _: Direction) -> bool { true }

    fn on_event(&mut self, event: Event) -> EventResult {
        match event {
            Event::Mouse { offset, position, event } => match event {
                MouseEvent::WheelUp => {
                    self.scrollbase.scroll_up(1);
                    EventResult::Consumed(None)
                },
                MouseEvent::WheelDown => {
                    self.scrollbase.scroll_down(1);
                    EventResult::Consumed(None)
                },
                MouseEvent::Press(MouseButton::Left)=> {
                    let mut pos = position.saturating_sub(offset);

                    if pos.y == 0 {
                        self.click_header(pos.x);
                    }

                    pos.y = pos.y.saturating_sub(2);

                    if self.scrollbase.content_height > self.scrollbase.view_height {
                        if self.scrollbase.start_drag(pos, self.width()) {
                            return EventResult::Consumed(None);
                        }
                    }

                    if pos.y < self.scrollbase.view_height {
                        let i = pos.y + self.scrollbase.start_line;
                        if let Some(row) = self.data.read().unwrap().rows().get(i) {
                            self.selected = Some(*row);
                            return EventResult::Consumed(None);
                        }
                    }

                    EventResult::Ignored
                },
                MouseEvent::Hold(MouseButton::Left) => {
                    let mut pos = position.saturating_sub(offset);
                    pos.y = pos.y.saturating_sub(2);
                    self.scrollbase.drag(pos);
                    EventResult::Consumed(None)
                },
                MouseEvent::Release(MouseButton::Left) => {
                    self.scrollbase.release_grab();
                    EventResult::Consumed(None)
                }
                _ => EventResult::Ignored,
            },
            _ => EventResult::Ignored,
        }
    }
}
