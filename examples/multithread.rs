/*
 * Copyright (c) 2018 Boucher, Antoni <bouanto@zoho.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy of
 * this software and associated documentation files (the "Software"), to deal in
 * the Software without restriction, including without limitation the rights to
 * use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
 * the Software, and to permit persons to whom the Software is furnished to do so,
 * subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
 * FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
 * COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
 * IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
 * CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */

extern crate gtk;
#[macro_use]
extern crate relm;
#[macro_use]
extern crate relm_derive;
#[cfg_attr(test, macro_use)]
extern crate relm_test;

use std::thread;
use std::time::Duration;

use gtk::{
    Inhibit,
    LabelExt,
    OrientableExt,
    WidgetExt,
};
use gtk::Orientation::Vertical;
use relm::{
    Channel,
    Loop,
    Relm,
    Widget,
};
use relm_derive::widget;

use self::Msg::*;

pub struct Model {
    text: String,
}

#[derive(Clone, Msg)]
pub enum Msg {
    Quit,
    Value(i32, usize),
}

#[widget]
impl Widget for Win {
    fn model(relm: &Relm<Self>, _: ()) -> Model {
        let stream = relm.stream().clone();
        let event_loop = Loop::default();
        let channel_entry = event_loop.reserve();
        // Create a channel to be able to send a message from another thread.
        let (channel, sender) = Channel::new(move |num| {
            // This closure is executed whenever a message is received from the sender.
            // We send a message to the current widget.
            stream.emit(Value(num, channel_entry));
        });
        event_loop.set_stream(channel_entry, channel);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            // Send a message from the other thread.
            // The value 42 will be received as the num parameter in the above closure.
            sender.send(42).expect("send message");
        });
        Model {
            text: "Computing...".to_string(),
        }
    }

    fn update(&mut self, event: Msg) {
        let event_loop = Loop::default();
        match event {
            Quit => Loop::quit(),
            Value(num, channel_entry) => {
                self.model.text = num.to_string();
                event_loop.remove_stream(channel_entry);
            },
        }
    }

    view! {
        gtk::Window {
            gtk::Box {
                orientation: Vertical,
                gtk::Label {
                    text: &self.model.text,
                },
            },
            delete_event(_, _) => (Quit, Inhibit(false)),
        }
    }
}

fn main() {
    Win::run(()).expect("Win::run failed");
}

#[cfg(test)]
mod tests {
    use relm;

    use Msg::Value;
    use Win;

    #[test]
    fn channel() {
        let (component, _widgets) = relm::init_test::<Win>(()).expect("init_test failed");
        let observer = relm_observer_new!(component, Value(_));
        relm_observer_wait!(let Value(value) = observer);
        assert_eq!(value, 42);
    }
}
