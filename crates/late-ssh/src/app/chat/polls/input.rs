use crate::app::{
    common::{
        primitives::Banner,
        textarea_input::{EditOutcome, handle_single_line_edit},
    },
    input::ParsedInput,
    state::App,
};

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B) => {
            handle_escape(app);
        }
        ParsedInput::Byte(b'\t') | ParsedInput::Arrow(b'B') => {
            app.poll_modal_state.move_focus(1);
        }
        ParsedInput::BackTab | ParsedInput::Arrow(b'A') => {
            app.poll_modal_state.move_focus(-1);
        }
        ParsedInput::Arrow(b'C')
        | ParsedInput::Byte(b'l' | b'L')
        | ParsedInput::Char('l' | 'L')
            if app.poll_modal_state.focus() == super::state::PollField::Duration =>
        {
            app.poll_modal_state.move_duration(1);
        }
        ParsedInput::Arrow(b'D')
        | ParsedInput::Byte(b'h' | b'H')
        | ParsedInput::Char('h' | 'H')
            if app.poll_modal_state.focus() == super::state::PollField::Duration =>
        {
            app.poll_modal_state.move_duration(-1);
        }
        ParsedInput::Byte(b'1') | ParsedInput::Char('1')
            if app.poll_modal_state.focus() == super::state::PollField::Duration =>
        {
            app.poll_modal_state.set_duration_index(0);
        }
        ParsedInput::Byte(b'2') | ParsedInput::Char('2')
            if app.poll_modal_state.focus() == super::state::PollField::Duration =>
        {
            app.poll_modal_state.set_duration_index(1);
        }
        ParsedInput::Byte(b'3') | ParsedInput::Char('3')
            if app.poll_modal_state.focus() == super::state::PollField::Duration =>
        {
            app.poll_modal_state.set_duration_index(2);
        }
        ParsedInput::Byte(b'\r' | b'\n') | ParsedInput::Char('\r' | '\n')
            if app.poll_modal_state.focus() == super::state::PollField::Duration =>
        {
            submit(app);
        }
        _ if app.poll_modal_state.focus() == super::state::PollField::Duration => {}
        event => {
            let max_chars = app.poll_modal_state.focused_max_chars();
            let outcome = handle_single_line_edit(
                app.poll_modal_state.focused_input_mut(),
                &event,
                max_chars,
            );
            match outcome {
                EditOutcome::Submit => submit(app),
                EditOutcome::Cancel => handle_escape(app),
                EditOutcome::Handled | EditOutcome::Ignored => {}
            }
        }
    }
}

pub(crate) fn handle_escape(app: &mut App) {
    app.poll_modal_state.close();
    app.show_poll_modal = false;
}

fn submit(app: &mut App) {
    match app.poll_modal_state.submit() {
        Ok(submit) => {
            app.chat.create_poll(
                submit.room_id,
                submit.question,
                submit.options,
                submit.duration_secs,
            );
            app.poll_modal_state.close();
            app.show_poll_modal = false;
            app.banner = Some(Banner::success("Starting poll..."));
        }
        Err(message) => {
            app.banner = Some(Banner::error(&message));
        }
    }
}
