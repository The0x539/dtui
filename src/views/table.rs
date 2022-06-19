use std::cmp::Ordering;
use std::ops::DerefMut;
use std::sync::{Arc, RwLock};

use cursive::direction::Direction;
use cursive::event::{Callback, Event, EventResult, MouseButton, MouseEvent};
use cursive::view::{scroll, CannotFocus};
use cursive::Printer;
use cursive::Vec2;
use cursive::View;

pub(crate) trait TableViewData: Default {
    type Column: Copy + Eq + AsRef<str>;
    type RowIndex: Copy + Eq;
    type RowValue;
    type Rows: DerefMut<Target = [Self::RowIndex]> + Default;

    const SHOULD_GROW_TO_FIT: bool = false;

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

    fn compare_rows(&self, a: &Self::RowIndex, b: &Self::RowIndex) -> Ordering;

    fn sort_unstable(&mut self) {
        let mut rows = std::mem::take(self.rows_mut());
        rows.sort_unstable_by(|a, b| self.compare_rows(a, b));
        self.set_rows(rows);
    }

    fn sort_stable(&mut self) {
        let mut rows = std::mem::take(self.rows_mut());
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

    fn get_row_value<'a>(&'a self, index: &'a Self::RowIndex) -> &'a Self::RowValue;

    fn draw_cell(&self, printer: &Printer, row: &Self::RowValue, column: Self::Column);

    fn draw_row(&self, printer: &Printer, columns: &[(Self::Column, usize)], row: &Self::RowValue) {
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
        sort_column = self.$col:ident;
        rows = self.$rows:ident;
        descending_sort = self.$sort:ident;
    ) => {
        fn sort_column(&self) -> Self::Column {
            self.$col
        }
        fn descending_sort(&self) -> bool {
            self.$sort
        }
        fn rows(&self) -> &Self::Rows {
            &self.$rows
        }
        fn rows_mut(&mut self) -> &mut Self::Rows {
            &mut self.$rows
        }
        fn set_rows(&mut self, val: Self::Rows) {
            self.$rows = val;
        }
    };
}

pub(super) trait TableCallback<T: TableViewData>:
    Fn(&mut T, &T::RowIndex, Vec2, Vec2) -> Callback + 'static
{
}
impl<T: TableViewData, F: Fn(&mut T, &T::RowIndex, Vec2, Vec2) -> Callback + 'static>
    TableCallback<T> for F
{
}
type BoxedTableCallback<T> = Box<dyn TableCallback<T>>;

pub(crate) struct TableView<T: TableViewData> {
    data: Arc<RwLock<T>>,
    columns: Vec<(T::Column, usize)>,
    scroll_core: scroll::Core,
    selected: Option<T::RowIndex>,
    double_click_primed: bool,
    on_selection_change: Option<BoxedTableCallback<T>>,
    on_double_click: Option<BoxedTableCallback<T>>,
    on_right_click: Option<BoxedTableCallback<T>>,
}

impl<T: TableViewData> TableView<T> {
    pub fn new(columns: Vec<(T::Column, usize)>) -> Self {
        Self {
            data: Arc::new(RwLock::new(T::default())),
            columns,
            scroll_core: scroll::Core::default(),
            selected: None,
            double_click_primed: false,
            on_selection_change: None,
            on_double_click: None,
            on_right_click: None,
        }
    }

    pub fn get_data(&self) -> Arc<RwLock<T>> {
        self.data.clone()
    }

    pub fn get_selection(&self) -> Option<&T::RowIndex> {
        self.selected.as_ref()
    }

    pub(super) fn set_on_selection_change(&mut self, f: impl TableCallback<T>) {
        self.on_selection_change = Some(Box::new(f));
    }

    pub(super) fn set_on_double_click(&mut self, f: impl TableCallback<T>) {
        self.on_double_click = Some(Box::new(f));
    }

    pub(super) fn set_on_right_click(&mut self, f: impl TableCallback<T>) {
        self.on_right_click = Some(Box::new(f));
    }

    fn click_header(&mut self, mut x: usize) -> EventResult {
        for (column, width) in &self.columns {
            if x < *width {
                self.data.write().unwrap().click_column(*column);
                return EventResult::Consumed(None);
            } else if x == *width {
                // a column separator was clicked; do nothing
                return EventResult::Ignored;
            }
            x -= width + 1;
        }
        return EventResult::Ignored;
    }

    fn width(&self) -> usize {
        self.columns
            .iter()
            .map(|(_, w)| w + 1)
            .sum::<usize>()
            .saturating_sub(1)
    }

    fn run_cb(
        res: EventResult,
        cb: &Option<BoxedTableCallback<T>>,
        data: &mut T,
        row: &T::RowIndex,
        position: Vec2,
        offset: Vec2,
    ) -> EventResult {
        if let Some(f) = cb {
            let cb = f(data, row, position, offset);
            res.and(EventResult::Consumed(Some(cb)))
        } else {
            res
        }
    }
}

impl<T: TableViewData> scroll::Scroller for TableView<T> {
    fn get_scroller(&self) -> &scroll::Core {
        &self.scroll_core
    }

    fn get_scroller_mut(&mut self) -> &mut scroll::Core {
        &mut self.scroll_core
    }
}

impl<T: TableViewData> View for TableView<T>
where
    Self: 'static,
{
    fn draw(&self, printer: &Printer) {
        let Vec2 { x: w, y: h } = printer.size;

        let data = self.data.read().unwrap();

        let mut x = 0;
        for (column, width) in &self.columns {
            let mut name = String::from(column.as_ref());

            if *column == data.sort_column() {
                let c = if data.descending_sort() {
                    " ▼"
                } else {
                    " ▲"
                };
                name.push_str(c);
            }

            printer.cropped((x + width, 1)).print((x, 0), &name);
            printer.print_hline((x, 1), *width, "─");
            x += width;
            if x == w {
                printer.print((0, 1), "╶");
                printer.print((x - 1, 1), "╴");
                break;
            }
            printer.print_vline((x, 0), h, "│");
            printer.print((x, 1), "┼");
            x += 1;
        }

        scroll::draw_lines(self, &printer.offset((0, 2)), |this, p, i| {
            if let Some(row) = data.rows().get(i) {
                p.with_selection(this.selected == Some(*row), |p| {
                    data.draw_row(p, &this.columns, data.get_row_value(row))
                });
            }
        });
    }

    fn required_size(&mut self, constraint: Vec2) -> Vec2 {
        let data_constraint = constraint.saturating_sub((0, 2));
        let data_requirement = scroll::required_size(
            self,
            data_constraint,
            true, // TODO: figure out what's up with this
            |this, constraint| (constraint.x, this.data.read().unwrap().rows().len()).into(),
        );
        let mut requirement = data_requirement + (0, 2);

        if T::SHOULD_GROW_TO_FIT {
            // Take up all available vertical space.
            // Flexbox is hard and this seems to work for what I need.
            //
            // BUG: If a tab is tall enough (easy to achieve for the Files and Peers tabs),
            // then the pane at the bottom will somehow crowd out the status bar.
            // TODO: Find to find a way to fix this.
            requirement.y = requirement.y.max(constraint.y);
        };
        requirement
    }

    fn layout(&mut self, size: Vec2) {
        // Don't trust the scroll core to tell us how wide we are.
        // The presence of a scroll bar shouldn't change column width,
        // because it doesn't extend into the header.
        // Other code might need to be changed accordingly,
        // but if you have spare space in your last column, you're fine.
        let others_width = self.columns[1..].iter().map(|(_, w)| w + 1).sum::<usize>();
        self.columns[0].1 = size.x - others_width;

        let data_size = size.checked_sub((0, 2)).expect("bar");
        scroll::layout(
            self,
            data_size,
            true, // TODO: when do we need to relayout?
            |_this, _size| (),
            |this, constraint| (constraint.x, this.data.read().unwrap().rows().len()).into(),
        );
    }

    fn take_focus(&mut self, _: Direction) -> Result<EventResult, CannotFocus> {
        Ok(EventResult::Consumed(None))
    }

    fn on_event(&mut self, event: Event) -> EventResult {
        // Un-prime double click on anything appropriate
        match event {
            Event::Mouse {
                position,
                offset,
                event,
            } => {
                if position.saturating_sub(offset).y < 2 {
                    self.double_click_primed = false;
                } else if event.button() != Some(MouseButton::Left) {
                    self.double_click_primed = false;
                }
            }
            Event::Refresh | Event::WindowResize => (),
            _ => self.double_click_primed = false,
        }

        match event {
            Event::Mouse {
                offset,
                position,
                event,
            } => match event {
                MouseEvent::WheelUp => {
                    self.scroll_core.scroll_up(1);
                    return EventResult::Consumed(None);
                }
                MouseEvent::WheelDown => {
                    self.scroll_core.scroll_down(1);
                    return EventResult::Consumed(None);
                }
                MouseEvent::Press(MouseButton::Left) => {
                    let mut pos = position.saturating_sub(offset);

                    if pos.y == 0 {
                        return self.click_header(pos.x);
                    } else if pos.y == 1 {
                        return EventResult::Ignored;
                    }

                    pos.y = pos.y.saturating_sub(2);

                    let core = &mut self.scroll_core;

                    if core.inner_size().y > core.last_outer_size().y
                        //&& pos.x == self_width
                        && core.start_drag(pos)
                    {
                        return EventResult::Consumed(None);
                    }

                    if pos.y < core.last_outer_size().y {
                        let i = pos.y + core.content_viewport().top();
                        let mut data = self.data.write().unwrap();
                        if let Some(&row) = data.rows().get(i) {
                            let mut res = EventResult::Consumed(None);

                            let selection_changed = self.selected != Some(row);
                            let double_clicked = self.double_click_primed && !selection_changed;

                            self.double_click_primed = !double_clicked;
                            self.selected = Some(row);

                            if selection_changed {
                                res = Self::run_cb(
                                    res,
                                    &self.on_selection_change,
                                    &mut data,
                                    &row,
                                    position,
                                    offset,
                                );
                            } else if double_clicked {
                                res = Self::run_cb(
                                    res,
                                    &self.on_double_click,
                                    &mut data,
                                    &row,
                                    position,
                                    offset,
                                );
                            }

                            return res;
                        }
                    }
                }
                MouseEvent::Press(MouseButton::Right) if position.y >= offset.y + 2 => {
                    let pos = position.saturating_sub(offset + (0, 2));
                    let i = pos.y + self.scroll_core.content_viewport().top();
                    let mut data = self.data.write().unwrap();
                    if let Some(&row) = data.rows().get(i) {
                        let mut res = EventResult::Consumed(None);
                        if self.selected != Some(row) {
                            self.selected = Some(row);
                            res = Self::run_cb(
                                res,
                                &self.on_selection_change,
                                &mut data,
                                &row,
                                position,
                                offset,
                            );
                        }
                        return Self::run_cb(
                            res,
                            &self.on_right_click,
                            &mut data,
                            &row,
                            position,
                            offset,
                        );
                    }
                }
                MouseEvent::Hold(MouseButton::Left) if position.y >= offset.y + 2 => {
                    let pos = position.saturating_sub(offset + (0, 2));
                    self.scroll_core.drag(pos);
                    self.double_click_primed = false;
                    return EventResult::Consumed(None);
                }
                MouseEvent::Release(MouseButton::Left) => {
                    self.scroll_core.release_grab();
                    if position.y < offset.y + 2 || position.x == self.width() {
                        self.double_click_primed = false;
                    } else {
                        let pos = position.saturating_sub(offset + (0, 2));
                        let i = pos.y + self.scroll_core.content_viewport().top();
                        let data = self.data.read().unwrap();
                        self.double_click_primed &= self.selected.as_ref() == data.rows().get(i);
                    }

                    return EventResult::Consumed(None);
                }
                _ => (),
            },
            _ => (),
        }

        EventResult::Ignored
    }
}
