use ratatui::backend::{Backend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use std::io;

use crate::proto::{CellDiff, CursorShape, GfxCmd, ScreenUpdate};

/// custom ratatui backend capturing frame cell diffs for client transmission.
pub struct ClientBackend {
    width: u16,
    height: u16,
    cursor_pos: Option<(u16, u16)>,
    cursor_visible: bool,
    cursor_shape: CursorShape,
    pending_diffs: Vec<CellDiff>,
    pending_gfx: Vec<GfxCmd>,
    gfx_seen_through: u64,
    full_redraw: bool,
    pending_reset: bool,
}

impl ClientBackend {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cursor_pos: None,
            cursor_visible: true,
            cursor_shape: CursorShape::Default,
            pending_diffs: Vec::new(),
            pending_gfx: Vec::new(),
            gfx_seen_through: 0,
            full_redraw: true,
            pending_reset: false,
        }
    }

    /// queues graphics commands to be emitted with the next update.
    ///
    /// commands whose `seq` is not newer than the last emitted sequence are
    /// dropped, unless the client is about to perform a full redraw (in which
    /// case everything is re-emitted to rebuild pixels after an ed clear) or
    /// the caller passes `force_reemit` (e.g. anchored graphics moved and
    /// must be re-placed by the client without a full redraw).
    pub fn set_pending_gfx(&mut self, items: Vec<GfxCmd>, force_reemit: bool) {
        if self.full_redraw || force_reemit {
            self.gfx_seen_through = 0;
        }
        // every emission carries a fresh global sequence, so the "not newer
        // than the last one" filter only drops genuine duplicates. it used
        // to drop re-placements as well, because the server sent the
        // per-store command sequence (small, reused) instead of the global
        // one - the image then never reached the client.
        let mut kept = Vec::with_capacity(items.len());
        for item in items {
            if item.seq > self.gfx_seen_through {
                kept.push(item);
            }
        }
        if let Some(max_seq) = kept.iter().map(|g| g.seq).max() {
            self.gfx_seen_through = self.gfx_seen_through.max(max_seq);
        }
        self.pending_gfx.extend(kept);
    }

    /// queues a DECSTR soft reset to be emitted before the next update.
    pub fn set_pending_reset(&mut self) {
        self.pending_reset = true;
    }

    /// takes captured diffs and constructs an outgoing screenupdate message.
    pub fn take_screen_update(&mut self) -> ScreenUpdate {
        let update = ScreenUpdate {
            cols: self.width,
            rows: self.height,
            cursor_pos: self.cursor_pos,
            cursor_visible: self.cursor_visible,
            cursor_shape: self.cursor_shape,
            cells: std::mem::take(&mut self.pending_diffs),
            full_redraw: self.full_redraw,
            gfx: std::mem::take(&mut self.pending_gfx),
            reset: std::mem::take(&mut self.pending_reset),
        };
        self.full_redraw = false;
        update
    }

    pub fn set_cursor_shape(&mut self, shape: CursorShape) {
        self.cursor_shape = shape;
    }

    pub fn set_full_redraw(&mut self, full: bool) {
        self.full_redraw = full;
    }

    pub fn set_dimensions(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.full_redraw = true;
    }
}

impl Backend for ClientBackend {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let to_proto = |c: ratatui::style::Color| -> crate::proto::ProtoColor {
            use crate::proto::ProtoColor;
            use ratatui::style::Color;
            match c {
                Color::Reset => ProtoColor::Default,
                Color::Black => ProtoColor::Indexed(0),
                Color::Red => ProtoColor::Indexed(1),
                Color::Green => ProtoColor::Indexed(2),
                Color::Yellow => ProtoColor::Indexed(3),
                Color::Blue => ProtoColor::Indexed(4),
                Color::Magenta => ProtoColor::Indexed(5),
                Color::Cyan => ProtoColor::Indexed(6),
                Color::Gray => ProtoColor::Indexed(7),
                Color::DarkGray => ProtoColor::Indexed(8),
                Color::LightRed => ProtoColor::Indexed(9),
                Color::LightGreen => ProtoColor::Indexed(10),
                Color::LightYellow => ProtoColor::Indexed(11),
                Color::LightBlue => ProtoColor::Indexed(12),
                Color::LightMagenta => ProtoColor::Indexed(13),
                Color::LightCyan => ProtoColor::Indexed(14),
                Color::White => ProtoColor::Indexed(15),
                Color::Indexed(i) => ProtoColor::Indexed(i),
                Color::Rgb(r, g, b) => ProtoColor::Rgb(r, g, b),
            }
        };
        for (x, y, cell) in content {
            let (fg, bg, underline_color) = (
                to_proto(cell.fg),
                to_proto(cell.bg),
                to_proto(cell.underline_color),
            );

            let modifier = cell.modifier;
            self.pending_diffs.push(CellDiff {
                x,
                y,
                symbol: cell.symbol().to_string(),
                fg,
                bg,
                underline_color,
                bold: modifier.contains(ratatui::style::Modifier::BOLD),
                dim: modifier.contains(ratatui::style::Modifier::DIM),
                italic: modifier.contains(ratatui::style::Modifier::ITALIC),
                underline: modifier.contains(ratatui::style::Modifier::UNDERLINED),
                reverse: modifier.contains(ratatui::style::Modifier::REVERSED),
                strike: modifier.contains(ratatui::style::Modifier::CROSSED_OUT),
            });
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = false;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.cursor_visible = true;
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        let (x, y) = self.cursor_pos.unwrap_or((0, 0));
        Ok(Position::new(x, y))
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        let pos = position.into();
        self.cursor_pos = Some((pos.x, pos.y));
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.full_redraw = true;
        self.pending_diffs.clear();
        Ok(())
    }

    fn clear_region(
        &mut self,
        _clear_type: ratatui::backend::ClearType,
    ) -> Result<(), Self::Error> {
        self.full_redraw = true;
        self.pending_diffs.clear();
        Ok(())
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(Size::new(self.width, self.height))
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: Size::new(self.width, self.height),
            pixels: Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ClientBackend;
    use ratatui::backend::Backend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    #[test]
    fn draw_carries_underline_color_into_screen_diff() {
        let mut backend = ClientBackend::new(1, 1);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 1, 1));
        buffer[(0, 0)]
            .set_style(ratatui::style::Style::default().underline_color(Color::Rgb(17, 34, 51)));
        backend
            .draw(std::iter::once((0, 0, &buffer[(0, 0)])))
            .expect("cell draw succeeds");

        let update = backend.take_screen_update();
        assert_eq!(
            update.cells[0].underline_color,
            crate::proto::ProtoColor::Rgb(17, 34, 51)
        );
    }
}
