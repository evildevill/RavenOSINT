use std::path::PathBuf;

use rust_xlsxwriter::*;
use tracing::info;

use crate::error::RavenError;
use crate::reporter::Reporter;
use crate::types::*;

struct XlsxRow {
    username: String,
    site_name: String,
    url_user: String,
    status: QueryStatus,
    http_status: Option<u16>,
    query_time_ms: Option<u64>,
}

pub struct XlsxReporter {
    file_path: PathBuf,
    rows: Vec<XlsxRow>,
    print_all: bool,
}

impl XlsxReporter {
    pub fn new<P: Into<PathBuf>>(file_path: P, print_all: bool) -> Self {
        XlsxReporter {
            file_path: file_path.into(),
            rows: Vec::new(),
            print_all,
        }
    }
}

impl Reporter for XlsxReporter {
    fn write_search_start(&mut self, _username: &str) -> Result<(), RavenError> {
        self.rows.clear();
        Ok(())
    }

    fn write_result(&mut self, result: &QueryResult) -> Result<(), RavenError> {
        if !self.print_all && result.status != QueryStatus::Claimed {
            return Ok(());
        }

        self.rows.push(XlsxRow {
            username: result.username.clone(),
            site_name: result.site_name.clone(),
            url_user: result.site_url_user.clone(),
            status: result.status.clone(),
            http_status: result.http_status,
            query_time_ms: result.query_time_ms,
        });
        Ok(())
    }

    fn write_search_complete(&mut self, _results: &SearchResults) -> Result<(), RavenError> {
        Ok(())
    }

    fn finish(&mut self) -> Result<(), RavenError> {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();
        worksheet.set_name("Results")?;

        let header_fmt = Format::new()
            .set_bold()
            .set_font_color(Color::White)
            .set_background_color(Color::from("#333333"))
            .set_border(FormatBorder::Thin);

        let headers = [
            "Username",
            "Site Name",
            "URL",
            "Status",
            "HTTP Status",
            "Response Time (ms)",
        ];
        for (col, header) in headers.iter().enumerate() {
            worksheet.write_string_with_format(0, col as u16, *header, &header_fmt)?;
        }

        for (row_idx, data) in self.rows.iter().enumerate() {
            let excel_row = (row_idx + 1) as u32;

            let status_color = match data.status {
                QueryStatus::Claimed => Color::from("#27ae60"),
                QueryStatus::Available => Color::from("#e67e22"),
                QueryStatus::Unknown => Color::from("#e74c3c"),
                QueryStatus::Illegal => Color::from("#95a5a6"),
                QueryStatus::Waf => Color::from("#e74c3c"),
            };

            let status_fmt = Format::new().set_font_color(status_color).set_bold();

            let bg_color = if row_idx % 2 == 0 {
                "#ffffff"
            } else {
                "#f5f5f5"
            };
            let row_fmt = Format::new().set_background_color(Color::from(bg_color));

            worksheet.write_string_with_format(excel_row, 0, &data.username, &row_fmt)?;
            worksheet.write_string_with_format(excel_row, 1, &data.site_name, &row_fmt)?;

            if !data.url_user.is_empty() {
                worksheet.write_url_with_format(excel_row, 2, data.url_user.as_str(), &row_fmt)?;
            } else {
                worksheet.write_string_with_format(excel_row, 2, "", &row_fmt)?;
            }

            worksheet.write_string_with_format(excel_row, 3, &data.status.to_string(), &status_fmt)?;
            worksheet.write_string_with_format(
                excel_row,
                4,
                &data.http_status.map(|s| s.to_string()).unwrap_or_default(),
                &row_fmt,
            )?;
            worksheet.write_string_with_format(
                excel_row,
                5,
                &data.query_time_ms.map(|t| t.to_string()).unwrap_or_default(),
                &row_fmt,
            )?;
        }

        worksheet.autofit();
        workbook.save(&self.file_path)?;
        info!("XLSX report written to {}", self.file_path.display());
        Ok(())
    }
}
