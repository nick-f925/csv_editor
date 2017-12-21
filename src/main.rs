#[macro_use] extern crate cute;
#[macro_use] extern crate log;
extern crate argparse;
extern crate cursive;
extern crate env_logger;
extern crate csv;
extern crate cursive_table_view;

use argparse::{ArgumentParser, Store, Print};

use std::cmp::Ordering;
use std::io::prelude::*;
use std::io::BufReader;
use std::fs::File;
use std::path::{Path, PathBuf};

use cursive::Cursive;
use cursive::traits::*;
use cursive::align::HAlign;
use cursive::direction::Orientation;
use cursive::views::{Dialog, LinearLayout};
use cursive_table_view::{TableView, TableViewItem};

#[derive(Clone, Debug)]
struct Cell {
    value: String,
}

#[derive(Clone, Debug)]
struct Row {
    cells: Vec<Cell>,
    rowid: i64
}

#[derive(Debug)]
struct Table {
    header: Row,
    rows: Vec<Row>,
    num_cols: usize,
}

enum Error {
    FileError { filepath: PathBuf, why: ::std::io::Error },
    ExitCode(i32)
}

impl Error {
    fn get_message(&self) -> Option<String> {
        match self {
            &Error::FileError{ref filepath, ref why} => {
                let fp = filepath.as_os_str().to_string_lossy();
                Some(format!("could not open '{}': {}", fp, why.to_string()))
            },
            &Error::ExitCode(_) => None
        }
    }
    fn from_bad_file<P>(filepath: P, why: ::std::io::Error) -> Error
    where P: AsRef<Path>
    {
        Error::FileError{filepath: filepath.as_ref().to_path_buf(), why: why}
    }
    fn print_error(&self) {
        match self.get_message() {
            Some(s) => eprintln!("fatal: {}", s),
            None => {}
        }
    }
    fn exit_code(&self) -> i32 {
        match self {
            &Error::ExitCode(i) => i,
            _ => 1
        }
    }
}

impl Cell {
    fn from_string(s: &str) -> Cell {
        Cell{value: String::from(s.trim())}
    }
    fn len(&self) -> usize {
        self.value.len()
    }
    fn set_value(&mut self, s: &str) {
        self.value.clear();
        self.value += s;
    }
}

impl Row {
    fn rowid(&self) -> i64 {
        self.rowid
    }
    fn rowid_str(&self) -> String {
        format!("{}", self.rowid)
    }
    fn num_cols(&self) -> usize {
        self.cells.len()
    }
    fn to_strings<'a>(&'a self) -> Vec<&'a str> {
        c![&cell.value[..], for cell in &self.cells]
    }
    fn add_cell(&mut self, cell: Cell) {
        self.cells.push(cell);
    }
    fn cell_width(&self, c: usize) -> usize {
        let cell: Option<&Cell> = self.cells.get(c);
        match cell {
            Some(cell) => cell.value.len(),
            None => 0
        }
    }
    fn try_get<'a>(&'a self, c: usize, missing: &'a str) -> &'a str {
        let cell: Option<&'a Cell> = self.cells.get(c);
        match cell {
            Some(cell) => &cell.value,
            None => missing
        }
    }
    fn from_line(s: String) -> Row {
        debug!("Row::from_line: {}", s.trim());
        let mut newself: Row = Row{cells: Vec::new(), rowid: -1};
        for term in s.split(',') {
            newself.add_cell(Cell::from_string(term));
        }
        return newself;
    }
}

impl Table {
    fn num_cols(&self) -> usize {
        self.num_cols
    }
    fn num_rows(&self) -> usize {
        self.rows.len()
    }
    fn sum_colwidth2(&self, min_cellwidth: usize, cell_padding: usize, header_padding: usize) -> usize {
        (0..self.num_cols()).map(|c| self.col_width2(c, min_cellwidth, cell_padding, header_padding)).sum()
    }
    fn fix_header_names(&mut self) {
        for (c, cell) in self.header.cells.iter_mut().enumerate() {
            if cell.len() == 0 {
                cell.set_value(format!("col:{}", c).as_str());
            }
        }
        for c in self.header.num_cols()..self.num_cols() {
            self.header.add_cell(Cell::from_string(format!("col:{}", c).as_str()));
        }
        assert_eq!(self.header.num_cols(), self.num_cols());
    }
    fn header_names<'a>(&'a self) -> Vec<&'a str> {
        self.header.to_strings()
    }
    fn add_line(&mut self, mut row: Row) {
        self.num_cols = self.num_cols.max(row.num_cols());
        row.rowid = self.rows.len() as i64;
        self.rows.push(row);
    }
    fn rowid_width(&self) -> usize {
        match self.rows.last() {
            Some(row) => row.rowid_str().len(),
            None => 0
        }
    }
    fn col_width(&self, c: usize) -> usize {
        let w: Option<usize> = self.rows.iter().map(|r| r.cell_width(c)).max();
        w.unwrap_or(0)
    }
    fn col_width2(&self, c: usize, cell_minwidth: usize, cell_padding: usize, header_padding: usize) -> usize {
        let w = self.col_width(c).max(cell_minwidth) + cell_padding;
        let w2 = self.header.cells[c].len() + header_padding;
        w.max(w2)
    }
    fn from_filepath<P>(filepath: P) -> Result<Table, Error>
    where P: AsRef<Path>
    {
        let file = match File::open(&filepath) {
            Err(why) => return Err(Error::from_bad_file(filepath, why)),
            Ok(file) => file
        };
        let mut buf = BufReader::new(file);
        let mut header: String = String::new();
        buf.read_line(&mut header).expect("failed to read from file");
        let mut newself: Table = Table {
            header: Row::from_line(header),
            rows: Vec::new(),
            num_cols: 0
        };
        for ln in buf.lines().map(|ln| ln.unwrap()) {
            if ln.len() > 0 {
                newself.add_line(Row::from_line(ln))
            }
        }
        newself.fix_header_names();
        Ok(newself)
    }
    fn create_table_view(&self) -> TableView<Row, BasicColumn> {
        let mut tv = TableView::<Row, BasicColumn>::new();
        let header_padding = 4;
        let null_width: usize = "<NULL>".len();
        let col_width: usize = self.rowid_width().max("rowid".len() + header_padding);
        tv = tv.column(BasicColumn::RowId, "rowid",
            |c| c.align(HAlign::Left).width(col_width));
        for (c, header_name) in self.header_names().iter().enumerate() {
            let header_width: usize = header_name.len() + header_padding;
            let col_width: usize = self.col_width(c).max(null_width).max(header_width);
            tv = tv.column(BasicColumn::ColumnPos{c: c}, header_name.clone(),
                            |c| c.align(HAlign::Left).width(col_width)
                    );
        }
        tv.items(self.rows.clone())
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
enum BasicColumn {
    RowId,
    ColumnPos{c: usize},
}

impl TableViewItem<BasicColumn> for Row {
    fn to_column(&self, column: BasicColumn) -> String {
        match column {
            BasicColumn::RowId => {
                format!("{}", self.rowid)
            },
            BasicColumn::ColumnPos{c} => {
                String::from(self.try_get(c, "<NULL>"))
            }
        }
    }
    fn cmp(&self, other: &Self, column: BasicColumn) -> Ordering
    where Self: Sized {
        match column {
            BasicColumn::RowId => {
                self.rowid.cmp(&other.rowid)
            },
            BasicColumn::ColumnPos{c} => {
                let lhs = &self.cells[c].value;
                let rhs = &other.cells[c].value;
                lhs.cmp(rhs)
            }
        }
    }
}

fn main() {
    fn body() -> Result<i32, Error> {
        let _ = env_logger::init();
        let mut filepath: String = String::new();
        {
            let mut ap = ArgumentParser::new();
            ap.set_description("view a csv file in a table (ncurses)");
            ap.add_option(&["-V", "--version"],
                Print(env!("CARGO_PKG_VERSION").to_string()), "Show Version");
            ap.refer(&mut filepath).required()
                .add_argument("file", Store, "filepath to .csv, use '-' to read from STDIN");
            match ap.parse_args() {
                Ok(()) => {}
                Err(x) => return Err(Error::ExitCode(x))
            }
        }
        let table = Table::from_filepath(filepath)?;
        info!("num_rows={}", table.rows.len());

        let mut siv = Cursive::new();
        let mut layout = LinearLayout::new(Orientation::Horizontal);
        let sum_colwidth: usize = table.sum_colwidth2("<NULL>".len(), 2, 2 + 4);
        let num_rows = table.num_rows() + 4;
        layout.add_child(table.create_table_view().min_size((sum_colwidth, num_rows)));
        siv.add_layer(Dialog::around(layout).title("csvView"));
        siv.run();
        Ok(0)
    }

    let exit_code: i32 = match body() {
        Err(e) => {
            e.print_error();
            e.exit_code()
        }
        Ok(i) => i
    };

    std::process::exit(exit_code);
}
