use iced_baseview::alignment::Vertical;
use nih_plug::prelude::Param;
use std::borrow::Borrow;

use crate::core::text::{Paragraph, Renderer as TextRenderer, Text};
use crate::core::widget::tree::{self, Tree};
use crate::core::{
    event, keyboard, layout, mouse, renderer, text, touch, Border, Clipboard, Color, Element,
    Event, Font, Layout, Length, Pixels, Point, Rectangle, Shell, Size, Vector, Widget,
};
use crate::core::widget::Id;
use crate::widget::text_input;
use crate::widget::text_input::TextInput;

use super::{util, ParamMessage};

/// When shift+dragging a parameter, one pixel dragged corresponds to this much change in the
/// noramlized parameter.
const GRANULAR_DRAG_MULTIPLIER: f32 = 0.1;

/// The thickness of this widget's borders.
const BORDER_WIDTH: f32 = 1.0;

/// Style configuration for how the slider should be displayed and behave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderStyle {
    /// Traditional horizontal slider (default)
    Horizontal,
    /// Vertical slider with up/down dragging
    Vertical,
    /// Rotary knob with configurable arc styles
    Rotary(RotaryStyle),
}

/// Style configuration for rotary sliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotaryStyle {
    /// Unipolar: 0% to 100%, arc grows from bottom
    Unipolar,
    /// Bipolar: 0% to 200% with center at 100%, arc grows from center
    Bipolar,
    /// Width-style like Ableton Live: 100% = 45% arc both sides, 200% = 270° arc, 0% = no arc
    Width,
    /// BipolarFullCircle: Full 360° rotation in both directions from a starting position,
    /// with +/- symbols in center. Can start from top or bottom.
    BipolarFullCircle { start_from_top: bool },
}

impl Default for SliderStyle {
    fn default() -> Self {
        Self::Horizontal
    }
}

impl SliderStyle {
    /// Calculate normalized value from cursor position based on slider style
    fn calculate_normalized_value(
        &self,
        bounds: &Rectangle,
        cursor_position: Point,
        _default_value: f32,
    ) -> f32 {
        match self {
            SliderStyle::Horizontal => util::remap_rect_x_coordinate(bounds, cursor_position.x),
            SliderStyle::Vertical => 1.0 - util::remap_rect_y_coordinate(bounds, cursor_position.y),
            SliderStyle::Rotary(_) => {
                // For rotary sliders, behave exactly like vertical sliders for mouse interaction
                1.0 - util::remap_rect_y_coordinate(bounds, cursor_position.y)
            }
        }
    }

    /// Calculate granular normalized value for shift+drag
    fn calculate_granular_normalized_value(
        &self,
        bounds: &Rectangle,
        start_pos: f32,
        current_pos: f32,
        start_value: f32,
        _default_value: f32,
    ) -> f32 {
        match self {
            SliderStyle::Horizontal => {
                let drag_delta = (current_pos - start_pos) * GRANULAR_DRAG_MULTIPLIER;
                let start_x = util::remap_rect_x_t(bounds, start_value);
                util::remap_rect_x_coordinate(bounds, start_x + drag_delta)
            }
            SliderStyle::Vertical => {
                let drag_delta = (current_pos - start_pos) * GRANULAR_DRAG_MULTIPLIER;
                let start_y = util::remap_rect_y_t(bounds, 1.0 - start_value);
                1.0 - util::remap_rect_y_coordinate(bounds, start_y + drag_delta)
            }
            SliderStyle::Rotary(_) => {
                // For rotary sliders, behave like vertical sliders with correct direction
                let drag_delta = (current_pos - start_pos) * GRANULAR_DRAG_MULTIPLIER;
                let start_y = util::remap_rect_y_t(bounds, 1.0 - start_value);
                1.0 - util::remap_rect_y_coordinate(bounds, start_y + drag_delta)
            }
        }
    }

    /// Draw the slider fill based on the style
    fn draw_slider_fill<Renderer>(
        &self,
        renderer: &mut Renderer,
        bounds: &Rectangle,
        current_value: f32,
        default_value: f32,
        fill_color: Color,
    ) where
        Renderer: renderer::Renderer,
    {
        match self {
            SliderStyle::Horizontal => {
                let fill_start_x = util::remap_rect_x_t(
                    bounds,
                    if (0.45..=0.55).contains(&default_value) {
                        default_value
                    } else {
                        0.0
                    },
                );
                let fill_end_x = util::remap_rect_x_t(bounds, current_value);

                let fill_rect = Rectangle {
                    x: fill_start_x.min(fill_end_x),
                    width: (fill_end_x - fill_start_x).abs(),
                    ..*bounds
                };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: fill_rect,
                        border: Border::default(),
                        ..Default::default()
                    },
                    fill_color,
                );
            }
            SliderStyle::Vertical => {
                let fill_start_y = util::remap_rect_y_t(
                    bounds,
                    if (0.45..=0.55).contains(&default_value) {
                        1.0 - default_value
                    } else {
                        1.0
                    },
                );
                let fill_end_y = util::remap_rect_y_t(bounds, 1.0 - current_value);

                let fill_rect = Rectangle {
                    y: fill_start_y.min(fill_end_y),
                    height: (fill_end_y - fill_start_y).abs(),
                    ..*bounds
                };
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: fill_rect,
                        border: Border::default(),
                        ..Default::default()
                    },
                    fill_color,
                );
            }
            SliderStyle::Rotary(_) => {
                // For rotary sliders, we don't draw any background fill
                // The arc rendering is done separately in the draw method
            }
        }
    }
}

/// Helper function to draw an arc using small circular dots
fn draw_arc_with_lines<Renderer>(
    renderer: &mut Renderer,
    center: Point,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    color: Color,
    dot_size: f32,
) where
    Renderer: renderer::Renderer,
{
    const DOTS: i32 = 48; // Number of dots to create smooth arc appearance
    let angle_step = (end_angle - start_angle) / DOTS as f32;

    // Draw small circular dots along the arc path
    for i in 0..=DOTS {
        let angle = start_angle + i as f32 * angle_step;
        let x = center.x + radius * angle.cos();
        let y = center.y + radius * angle.sin();

        // Create a small circular quad for each dot
        let dot_bounds = Rectangle {
            x: x - dot_size / 2.0,
            y: y - dot_size / 2.0,
            width: dot_size,
            height: dot_size,
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: dot_bounds,
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: (dot_size / 2.0).into(),
                },
                ..Default::default()
            },
            color,
        );
    }
}

/// A slider that integrates with NIH-plug's [`Param`] types.
///
/// TODO: There are currently no styling options at all
/// TODO: Handle scrolling for steps (and shift+scroll for smaller steps?)
pub struct ParamSlider<'a, P: Param> {
    param: &'a P,

    width: Length,
    height: Length,
    text_size: Option<Pixels>,
    font: Option<Font>,
    style: SliderStyle,
    /// Size of the arc indicator dots for rotary sliders
    rotary_indicator_size: f32,
    /// Whether to position the text below the rotary slider instead of in the center
    rotary_text_below: bool,
    /// Padding between the rotary slider and the text when displayed below
    rotary_text_padding: f32,
    /// Color of the value indicator circle for rotary sliders
    rotary_value_color: Color,
    /// Whether to show the full circle path underneath the arc
    rotary_show_path: bool,
    /// Hover background color for both linear and rotary sliders
    hover_color: Color,
}

/// State for a [`ParamSlider`].
#[derive(Debug)]
struct State {
    keyboard_modifiers: keyboard::Modifiers,
    /// Will be set to `true` if we're dragging the parameter. Resetting the parameter or entering a
    /// text value should not initiate a drag.
    drag_active: bool,
    /// We keep track of the start coordinate and normalized value holding down Shift while dragging
    /// for higher precision dragging. This is a `None` value when granular dragging is not active.
    granular_drag_start_x_value: Option<(f32, f32)>,
    /// Track clicks for double clicks.
    last_click: Option<mouse::Click>,

    /// The text that's currently in the text input. If this is set to `None`, then the text input
    /// is not visible.
    text_input_value: Option<String>,
    text_input_id: Id,
}

impl Default for State {
    fn default() -> Self {
        Self {
            text_input_id: Id::unique(),
            keyboard_modifiers: Default::default(),
            drag_active: Default::default(),
            granular_drag_start_x_value: Default::default(),
            last_click: Default::default(),
            text_input_value: Default::default(),
        }
    }
}

/// An internal message for intercep- I mean handling output from the embedded [`TextInput`] widget.
#[derive(Debug, Clone)]
enum TextInputMessage {
    /// A new value was entered in the text input dialog.
    Value(String),
    /// Enter was pressed.
    Submit,
}

impl<'a, P: Param> ParamSlider<'a, P> {
    pub const DEFAULT_WIDTH: Length = Length::Fixed(180.0);
    pub const DEFAULT_HEIGHT: Length = Length::Fixed(30.0);

    /// Creates a new [`ParamSlider`] for the given parameter.
    pub fn new(param: &'a P) -> Self {
        Self {
            param,

            width: Self::DEFAULT_WIDTH,
            height: Self::DEFAULT_HEIGHT,
            text_size: None,
            font: None,
            style: SliderStyle::default(),
            rotary_indicator_size: 5.0,
            rotary_text_below: false,
            rotary_text_padding: 15.0,
            rotary_value_color: Color::from_rgb8(150, 150, 220), // Light blue with touch of purple
            rotary_show_path: true,
            hover_color: Color::from_linear_rgba(0.5, 0.5, 0.5, 0.1),
        }
    }

    /// Sets the width of the [`ParamSlider`].
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// Sets the height of the [`ParamSlider`].
    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }

    /// Sets the text size of the [`ParamSlider`].
    pub fn text_size(mut self, size: Pixels) -> Self {
        self.text_size = Some(size);
        self
    }

    /// Sets the font of the [`ParamSlider`].
    pub fn font(mut self, font: Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Sets the style of the [`ParamSlider`].
    pub fn style(mut self, style: SliderStyle) -> Self {
        self.style = style;
        self
    }

    /// Creates a vertical slider.
    pub fn vertical(mut self) -> Self {
        self.style = SliderStyle::Vertical;
        self
    }

    /// Creates a rotary slider with the specified style.
    pub fn rotary(mut self, rotary_style: RotaryStyle) -> Self {
        self.style = SliderStyle::Rotary(rotary_style);
        self
    }

    /// Sets the size of the indicator dots for rotary sliders.
    pub fn rotary_indicator_size(mut self, size: f32) -> Self {
        self.rotary_indicator_size = size;
        self
    }

    /// Sets whether to position the text below the rotary slider instead of in the center.
    pub fn rotary_text_below(mut self, below: bool) -> Self {
        self.rotary_text_below = below;
        self
    }

    /// Sets the padding between the rotary slider and the text when displayed below.
    pub fn rotary_text_padding(mut self, padding: f32) -> Self {
        self.rotary_text_padding = padding;
        self
    }

    /// Sets the color of the value indicator circle for rotary sliders.
    pub fn rotary_value_color(mut self, color: Color) -> Self {
        self.rotary_value_color = color;
        self
    }

    /// Sets whether to show the full circle path underneath the arc.
    pub fn rotary_show_path(mut self, show: bool) -> Self {
        self.rotary_show_path = show;
        self
    }

    /// Sets the hover background color for both linear and rotary sliders.
    pub fn hover_color(mut self, color: Color) -> Self {
        self.hover_color = color;
        self
    }

    /// Create a temporary [`TextInput`] hooked up to [`State::text_input_value`] and outputting
    /// [`TextInputMessage`] messages and do something with it. This can be used to
    fn with_text_input<T, Theme, Renderer, BorrowedRenderer, F>(
        &self,
        layout: Layout,
        renderer: BorrowedRenderer,
        current_value: &str,
        state: &State,
        f: F,
    ) -> T
    where
        F: FnOnce(TextInput<'_, TextInputMessage, Theme, Renderer>, Layout, BorrowedRenderer) -> T,
        Theme: text_input::Catalog,
        Renderer: TextRenderer,
        Renderer::Font: From<crate::Font>,
        BorrowedRenderer: Borrow<Renderer>,
    {
        let font = self
            .font
            .map(Renderer::Font::from)
            .unwrap_or_else(|| renderer.borrow().default_font());

        let text_size = self
            .text_size
            .unwrap_or_else(|| renderer.borrow().default_size());
        let text_width = Renderer::Paragraph::with_text(Text {
            content: current_value,
            bounds: layout.bounds().size(),
            size: text_size,
            font,
            line_height: Default::default(),
            align_x: text::Alignment::Center,
            align_y: Vertical::Center,
            shaping: Default::default(),
            wrapping: Default::default(),
            ellipsis: Default::default(),
            hint_factor: None,
        })
        .min_width();

        let text_input = text_input("", current_value)
            .id(state.text_input_id.clone())
            .font(font)
            .size(text_size)
            .width(text_width)
            .on_input(TextInputMessage::Value)
            .on_submit(TextInputMessage::Submit);

        // Make sure to not draw over the borders, and center the text
        let offset_node = layout::Node::with_children(
            Size {
                width: text_width,
                height: layout.bounds().shrink(BORDER_WIDTH).size().height,
            },
            vec![layout::Node::new(layout.bounds().size())],
        );
        let offset_layout = Layout::with_offset(
            Vector {
                x: layout.bounds().center_x() - (text_width / 2.0),
                y: layout.position().y + BORDER_WIDTH,
            },
            &offset_node,
        );

        f(text_input, offset_layout, renderer)
    }

    /// Set the normalized value for a parameter if that would change the parameter's plain value
    /// (to avoid unnecessary duplicate parameter changes). The begin- and end set parameter
    /// messages need to be sent before calling this function.
    fn set_normalized_value(&self, shell: &mut Shell<'_, ParamMessage>, normalized_value: f32) {
        // This snaps to the nearest plain value if the parameter is stepped in some way.
        // TODO: As an optimization, we could add a `const CONTINUOUS: bool` to the parameter to
        //       avoid this normalized->plain->normalized conversion for parameters that don't need
        //       it
        let plain_value = self.param.preview_plain(normalized_value);
        let current_plain_value = self.param.modulated_plain_value();
        if plain_value != current_plain_value {
            // For the aforementioned snapping
            let normalized_plain_value = self.param.preview_normalized(plain_value);
            shell.publish(ParamMessage::SetParameterNormalized(
                self.param.as_ptr(),
                normalized_plain_value,
            ));
        }
    }
}

impl<'a, P, Theme, Renderer> Widget<ParamMessage, Theme, Renderer> for ParamSlider<'a, P>
where
    P: Param,
    Theme: text_input::Catalog,
    Renderer: TextRenderer,
    Renderer::Font: From<crate::Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        let input = text_input::<TextInputMessage, Theme, Renderer>("", "");

        // One child to store text input state.
        vec![Tree {
            tag: input.tag(),
            state: input.state(),
            children: input.children(),
        }]
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, self.width, self.height)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();

        // Draw background and border based on style
        match self.style {
            SliderStyle::Rotary(_) => {
                // For rotary sliders, draw a subtle circular hover effect
                if cursor.is_over(bounds) || state.drag_active || state.text_input_value.is_some() {
                    let center = Point::new(bounds.center_x(), bounds.center_y());
                    let radius = (bounds.width.min(bounds.height)) / 2.0;

                    // Draw circular hover background
                    let hover_bounds = Rectangle {
                        x: center.x - radius,
                        y: center.y - radius,
                        width: radius * 2.0,
                        height: radius * 2.0,
                    };

                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: hover_bounds,
                            border: Border {
                                color: Color::TRANSPARENT,
                                width: 0.0,
                                radius: radius.into(),
                            },
                            ..Default::default()
                        },
                        self.hover_color,
                    );
                }
            }
            _ => {
                let background_color = if cursor.is_over(bounds)
                    || state.drag_active
                    || state.text_input_value.is_some()
                {
                    self.hover_color
                } else {
                    Color::TRANSPARENT
                };

                renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border {
                            color: Color::BLACK,
                            width: BORDER_WIDTH,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    },
                    background_color,
                );
            }
        }

        // Shrink bounds to inside of the border
        let bounds = bounds.shrink(BORDER_WIDTH);

        if let Some(current_value) = &state.text_input_value {
            self.with_text_input(
                layout,
                renderer,
                current_value,
                state,
                |text_input, layout, renderer| {
                    text_input.draw(
                        &tree.children[0],
                        renderer,
                        theme,
                        layout,
                        cursor,
                        None,
                        viewport,
                    );
                },
            );
        } else {
            // We'll visualize the difference between the current value and the default value if the
            // default value lies somewhere in the middle and the parameter is continuous. Otherwise
            // this appraoch looks a bit jarring.
            let current_value = self.param.modulated_normalized_value();
            let default_value = self.param.default_normalized_value();
            let fill_color = Color::from_rgb8(196, 196, 196);

            // Use the appropriate drawing method based on style
            let use_default_center =
                self.param.step_count().is_none() && (0.45..=0.55).contains(&default_value);
            let effective_default = if use_default_center {
                default_value
            } else {
                0.0
            };

            self.style.draw_slider_fill(
                renderer,
                &bounds,
                current_value,
                effective_default,
                fill_color,
            );

            // For rotary sliders, draw arcs using line segments
            if let SliderStyle::Rotary(rotary_style) = self.style {
                // Ensure consistent center calculation for all rotary styles to maintain baseline alignment
                let center = Point::new(
                    (bounds.x + bounds.width / 2.0).round(),
                    (bounds.y + bounds.height / 2.0).round(),
                );
                let radius = ((bounds.width.min(bounds.height) - BORDER_WIDTH * 4.0) / 2.0).round();

                // Draw style-specific path if enabled
                if self.rotary_show_path {
                    let path_color = Color::from_rgb8(230, 230, 230); // Very light grey

                    match rotary_style {
                        RotaryStyle::Unipolar => {
                            // Draw 270° arc from 135° to 405°
                            let start_angle = 3.0 * std::f32::consts::PI / 4.0;
                            let end_angle = start_angle + 3.0 * std::f32::consts::PI / 2.0;
                            draw_arc_with_lines(
                                renderer,
                                center,
                                radius,
                                start_angle,
                                end_angle,
                                path_color,
                                self.rotary_indicator_size,
                            );
                        }
                        RotaryStyle::Bipolar => {
                            // Draw 270° arc centered at bottom
                            let center_angle = 3.0 * std::f32::consts::PI / 2.0;
                            let start_angle = center_angle - 3.0 * std::f32::consts::PI / 4.0;
                            let end_angle = center_angle + 3.0 * std::f32::consts::PI / 4.0;
                            draw_arc_with_lines(
                                renderer,
                                center,
                                radius,
                                start_angle,
                                end_angle,
                                path_color,
                                self.rotary_indicator_size,
                            );
                        }
                        RotaryStyle::Width => {
                            // Draw maximum possible arc (240°) centered at bottom
                            let bottom_angle = 3.0 * std::f32::consts::PI / 2.0;
                            let max_extent =
                                std::f32::consts::PI / 4.0 + std::f32::consts::PI * 5.0 / 12.0;
                            draw_arc_with_lines(
                                renderer,
                                center,
                                radius,
                                bottom_angle - max_extent,
                                bottom_angle + max_extent,
                                path_color,
                                self.rotary_indicator_size,
                            );
                        }
                        RotaryStyle::BipolarFullCircle { .. } => {
                            // Draw full circle for BipolarFullCircle
                            draw_arc_with_lines(
                                renderer,
                                center,
                                radius,
                                0.0,
                                2.0 * std::f32::consts::PI,
                                path_color,
                                self.rotary_indicator_size,
                            );
                        }
                    }
                }

                // Draw arcs by connecting line segments
                match rotary_style {
                    RotaryStyle::Unipolar => {
                        if current_value > 0.0 {
                            // Use 270° range starting from 135° (top-left), rotated 90° anti-clockwise from 225°
                            let start_angle = 3.0 * std::f32::consts::PI / 4.0; // 135°
                            let end_angle =
                                start_angle + (current_value * 3.0 * std::f32::consts::PI / 2.0); // 135° to 405° (270° range)

                            draw_arc_with_lines(
                                renderer,
                                center,
                                radius,
                                start_angle,
                                end_angle,
                                fill_color,
                                self.rotary_indicator_size,
                            );
                        }
                    }
                    RotaryStyle::Bipolar => {
                        let center_angle = 3.0 * std::f32::consts::PI / 2.0; // 270° (bottom)
                        let current_angle =
                            center_angle + (current_value - 0.5) * 3.0 * std::f32::consts::PI / 2.0;

                        let start_angle = center_angle.min(current_angle);
                        let end_angle = center_angle.max(current_angle);

                        draw_arc_with_lines(
                            renderer,
                            center,
                            radius,
                            start_angle,
                            end_angle,
                            fill_color,
                            self.rotary_indicator_size,
                        );
                    }
                    RotaryStyle::Width => {
                        let bottom_angle = 3.0 * std::f32::consts::PI / 2.0; // 270°
                        let width_value = current_value * 2.0;

                        if width_value > 0.0 {
                            let arc_extent = if width_value <= 1.0 {
                                width_value * (std::f32::consts::PI / 4.0) // 0-45°
                            } else {
                                std::f32::consts::PI / 4.0
                                    + (width_value - 1.0) * (std::f32::consts::PI * 5.0 / 12.0)
                                // 45-120°
                            };

                            let start_angle = bottom_angle - arc_extent;
                            let end_angle = bottom_angle + arc_extent;

                            draw_arc_with_lines(
                                renderer,
                                center,
                                radius,
                                start_angle,
                                end_angle,
                                fill_color,
                                self.rotary_indicator_size,
                            );
                        }
                    }
                    RotaryStyle::BipolarFullCircle { start_from_top } => {
                        // Start at top (-90°) or bottom (90°) based on configuration
                        let start_angle = if start_from_top {
                            -std::f32::consts::PI / 2.0 // Top (6am)
                        } else {
                            std::f32::consts::PI / 2.0 // Bottom (6pm)
                        };

                        // Value is normalized 0-1, where 0.5 is center (neutral)
                        // Convert to -1 to 1 range where 0 is center
                        let centered_value = (current_value - 0.5) * 2.0;

                        if centered_value.abs() > 0.01 {
                            // Full 360° (2*PI) rotation for full range
                            let angle_range = centered_value.abs() * 2.0 * std::f32::consts::PI;

                            let (arc_start, arc_end) = if centered_value > 0.0 {
                                // Positive: clockwise from start
                                (start_angle, start_angle + angle_range)
                            } else {
                                // Negative: counter-clockwise from start
                                (start_angle - angle_range, start_angle)
                            };

                            draw_arc_with_lines(
                                renderer,
                                center,
                                radius,
                                arc_start,
                                arc_end,
                                fill_color,
                                self.rotary_indicator_size,
                            );
                        }
                    }
                }

                // Helper closure to draw an indicator at a given angle
                let draw_indicator = |renderer: &mut Renderer, angle: f32| {
                    let indicator_x = center.x + radius * angle.cos();
                    let indicator_y = center.y + radius * angle.sin();

                    // Draw shadow circle (slightly bigger and darker)
                    let shadow_color = Color::from_rgba8(0, 0, 0, 0.3);
                    let shadow_size = self.rotary_indicator_size + 2.0;
                    let shadow_bounds = Rectangle {
                        x: indicator_x - shadow_size / 2.0,
                        y: indicator_y - shadow_size / 2.0 + 1.0, // Offset slightly down
                        width: shadow_size,
                        height: shadow_size,
                    };

                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: shadow_bounds,
                            border: Border {
                                color: Color::TRANSPARENT,
                                width: 0.0,
                                radius: (shadow_size / 2.0).into(),
                            },
                            ..Default::default()
                        },
                        shadow_color,
                    );

                    // Draw value indicator circle
                    let indicator_bounds = Rectangle {
                        x: indicator_x - self.rotary_indicator_size / 2.0,
                        y: indicator_y - self.rotary_indicator_size / 2.0,
                        width: self.rotary_indicator_size,
                        height: self.rotary_indicator_size,
                    };

                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: indicator_bounds,
                            border: Border {
                                color: Color::TRANSPARENT,
                                width: 0.0,
                                radius: (self.rotary_indicator_size / 2.0).into(),
                            },
                            ..Default::default()
                        },
                        self.rotary_value_color,
                    );
                };

                // Draw value indicator(s) based on style
                match rotary_style {
                    RotaryStyle::Unipolar => {
                        // Start at 135° and rotate 270°
                        let start_angle = 3.0 * std::f32::consts::PI / 4.0;
                        let value_angle =
                            start_angle + (current_value * 3.0 * std::f32::consts::PI / 2.0);
                        draw_indicator(renderer, value_angle);
                    }
                    RotaryStyle::Bipolar => {
                        // Start at 270° (bottom) and rotate ±135°
                        let center_angle = 3.0 * std::f32::consts::PI / 2.0;
                        let value_angle =
                            center_angle + (current_value - 0.5) * 3.0 * std::f32::consts::PI / 2.0;
                        draw_indicator(renderer, value_angle);
                    }
                    RotaryStyle::Width => {
                        // For width style, show indicators at both edges of the arc
                        let bottom_angle = 3.0 * std::f32::consts::PI / 2.0;
                        let width_value = current_value * 2.0;

                        if width_value > 0.0 {
                            let arc_extent = if width_value <= 1.0 {
                                width_value * (std::f32::consts::PI / 4.0)
                            } else {
                                std::f32::consts::PI / 4.0
                                    + (width_value - 1.0) * (std::f32::consts::PI * 5.0 / 12.0)
                            };

                            // Draw indicators at both ends
                            draw_indicator(renderer, bottom_angle - arc_extent);
                            draw_indicator(renderer, bottom_angle + arc_extent);
                        } else {
                            // When value is 0, just draw one indicator at the bottom
                            draw_indicator(renderer, bottom_angle);
                        }
                    }
                    RotaryStyle::BipolarFullCircle { start_from_top } => {
                        let start_angle = if start_from_top {
                            -std::f32::consts::PI / 2.0
                        } else {
                            std::f32::consts::PI / 2.0
                        };
                        let centered_value = (current_value - 0.5) * 2.0;
                        let angle_range = centered_value * 2.0 * std::f32::consts::PI;
                        let value_angle = start_angle + angle_range;
                        draw_indicator(renderer, value_angle);
                    }
                }
            }

            // Draw the text label
            let display_value = self.param.to_string();
            let text_size = match self.style {
                SliderStyle::Rotary(_) => {
                    // Make text smaller for rotary sliders to avoid overlap with arc
                    let default_size = self.text_size.unwrap_or_else(|| renderer.default_size());
                    Pixels((default_size.0 * 0.8).max(8.0)) // 80% of default size, but at least 8px
                }
                _ => self.text_size.unwrap_or_else(|| renderer.default_size()),
            };
            let font = self
                .font
                .map(Renderer::Font::from)
                .unwrap_or_else(|| renderer.default_font());

            match self.style {
                SliderStyle::Rotary(rotary_style) => {
                    // For FullCircle style, draw +/- symbol
                    if let RotaryStyle::BipolarFullCircle { start_from_top: _ } = rotary_style {
                        // Draw +/- symbol in the center
                        let centered_value = (current_value - 0.5) * 2.0;
                        let symbol = if centered_value > 0.01 {
                            "+"
                        } else if centered_value < -0.01 {
                            "–" // Using en-dash for better visual
                        } else {
                            "" // No symbol at center
                        };

                        if !symbol.is_empty() {
                            let symbol_size = Pixels(text_size.0 * 1.5); // Make symbol larger
                            let symbol_bounds = Rectangle {
                                x: bounds.center_x(),
                                y: bounds.center_y(),
                                width: bounds.width,
                                height: bounds.height,
                            };

                            renderer.fill_text(
                                text::Text {
                                    content: symbol.to_string(),
                                    font: font,
                                    size: symbol_size,
                                    bounds: symbol_bounds.size(),
                                    align_x: text::Alignment::Center,
                                    align_y: Vertical::Center,
                                    line_height: text::LineHeight::Relative(1.0),
                                    shaping: Default::default(),
                                    wrapping: Default::default(),
                                    ellipsis: Default::default(),
                                    hint_factor: None,
                                },
                                symbol_bounds.position(),
                                fill_color,
                                *viewport,
                            );
                        }
                    }

                    let text_bounds = if self.rotary_text_below {
                        // Position text below the rotary circle
                        let radius = (bounds.width.min(bounds.height) - BORDER_WIDTH * 4.0) / 2.0;
                        Rectangle {
                            x: bounds.center_x(),
                            y: bounds.center_y() + radius + self.rotary_text_padding,
                            width: bounds.width,
                            height: text_size.0 + 4.0,
                        }
                    } else {
                        // Position text in the center
                        Rectangle {
                            x: bounds.center_x(),
                            y: bounds.center_y(),
                            ..bounds
                        }
                    };

                    // For rotary sliders, draw the text without clipping
                    renderer.fill_text(
                        text::Text {
                            content: display_value,
                            font: font,
                            size: text_size,
                            bounds: text_bounds.size(),
                            align_x: text::Alignment::Center,
                            align_y: Vertical::Center,
                            line_height: text::LineHeight::Relative(1.0),
                            shaping: Default::default(),
                            wrapping: Default::default(),
                            ellipsis: Default::default(),
                            hint_factor: None,
                        },
                        text_bounds.position(),
                        style.text_color,
                        *viewport,
                    );
                }
                _ => {
                    let text_bounds = Rectangle {
                        x: bounds.center_x(),
                        y: bounds.center_y(),
                        ..bounds
                    };

                    // For horizontal and vertical sliders, use the original clipped text approach
                    renderer.fill_text(
                        text::Text {
                            content: display_value.clone(),
                            font: font,
                            size: text_size,
                            bounds: text_bounds.size(),
                            align_x: text::Alignment::Center,
                            align_y: Vertical::Center,
                            line_height: text::LineHeight::Relative(1.0),
                            shaping: Default::default(),
                            wrapping: Default::default(),
                            ellipsis: Default::default(),
                            hint_factor: None,
                        },
                        text_bounds.position(),
                        style.text_color,
                        *viewport,
                    );

                    // Calculate fill rect for clipping (this is style-dependent)
                    let clip_rect = match self.style {
                        SliderStyle::Horizontal => {
                            let fill_start_x = util::remap_rect_x_t(&bounds, effective_default);
                            let fill_end_x = util::remap_rect_x_t(&bounds, current_value);
                            Rectangle {
                                x: fill_start_x.min(fill_end_x),
                                width: (fill_end_x - fill_start_x).abs(),
                                ..bounds
                            }
                        }
                        SliderStyle::Vertical => {
                            let fill_start_y =
                                util::remap_rect_y_t(&bounds, 1.0 - effective_default);
                            let fill_end_y = util::remap_rect_y_t(&bounds, 1.0 - current_value);
                            Rectangle {
                                y: fill_start_y.min(fill_end_y),
                                height: (fill_end_y - fill_start_y).abs(),
                                ..bounds
                            }
                        }
                        _ => bounds, // Won't reach here but need to satisfy compiler
                    };

                    // This will clip to the filled area
                    renderer.with_layer(clip_rect, |renderer| {
                        let filled_text_color = Color::from_rgb8(80, 80, 80);
                        renderer.fill_text(
                            text::Text {
                                content: display_value,
                                font: font,
                                size: text_size,
                                bounds: text_bounds.size(),
                                align_x: text::Alignment::Center,
                                align_y: Vertical::Center,
                                line_height: text::LineHeight::Relative(1.0),
                                shaping: Default::default(),
                                wrapping: Default::default(),
                                ellipsis: Default::default(),
                                hint_factor: None,
                            },
                            text_bounds.position(),
                            filled_text_color,
                            *viewport,
                        );
                    });
                }
            }
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_baseview::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        shell: &mut Shell<'_, ParamMessage>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        // The pressence of a value in `self.state.text_input_value` indicates that the field should
        // be focussed. The field handles defocussing by itself
        // FIMXE: This is super hacky, I have no idea how you can reuse the text input widget
        //        otherwise. Widgets are not supposed to handle messages from other widgets, but
        //        we'll do so anyways by using a special `TextInputMessage` type and our own
        //        `Shell`.
        let _text_input_status = if let Some(current_value) = &state.text_input_value {
            let event = event.clone();
            let mut messages = Vec::new();
            let mut text_input_shell = Shell::new(&mut messages);

            let status = self.with_text_input(
                layout,
                renderer,
                current_value,
                &state,
                |mut text_input: TextInput<TextInputMessage, Theme, Renderer>, layout, renderer| {
                    text_input.update(
                        &mut tree.children[0],
                        &event,
                        layout,
                        cursor,
                        renderer,
                        &mut text_input_shell,
                        viewport,
                    )
                },
            );

            // Check if text input is focused.
            let text_input_state = tree.children[0]
                .state
                .downcast_ref::<text_input::State<Renderer::Paragraph>>();

            // Pressing escape will unfocus the text field, so we should propagate that change in
            // our own model
            if text_input_state.is_focused() {
                for message in messages {
                    match message {
                        TextInputMessage::Value(s) => state.text_input_value = Some(s),
                        TextInputMessage::Submit => {
                            if let Some(normalized_value) = state
                                .text_input_value
                                .as_ref()
                                .and_then(|s| self.param.string_to_normalized_value(s))
                            {
                                shell.publish(ParamMessage::BeginSetParameter(self.param.as_ptr()));
                                self.set_normalized_value(shell, normalized_value);
                                shell.publish(ParamMessage::EndSetParameter(self.param.as_ptr()));
                            }

                            // And defocus the text input widget again
                            state.text_input_value = None;
                        }
                    }
                }
            } else {
                state.text_input_value = None;
            }

            status
        };

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                let bounds = layout.bounds();

                let Some(cursor_position) = cursor.position_over(bounds) else {
                    return;
                };

                let click =
                    mouse::Click::new(cursor_position, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);

                if state.keyboard_modifiers.alt() {
                    // Alt+click should not start a drag, instead it should show the text entry
                    // widget
                    state.drag_active = false;

                    let current_value = self.param.to_string();
                    state.text_input_value = Some(current_value.clone());

                    let text_input_state = tree.children[0]
                        .state
                        .downcast_mut::<text_input::State<Renderer::Paragraph>>();
                    text_input_state.select_all();
                    text_input_state.move_cursor_to_end();
                    text_input_state.focus();
                } else if state.keyboard_modifiers.command()
                    || matches!(click.kind(), mouse::click::Kind::Double)
                {
                    // Likewise resetting a parameter should not let you immediately drag it to a new value
                    state.drag_active = false;

                    shell.publish(ParamMessage::BeginSetParameter(self.param.as_ptr()));
                    self.set_normalized_value(shell, self.param.default_normalized_value());
                    shell.publish(ParamMessage::EndSetParameter(self.param.as_ptr()));
                } else if state.keyboard_modifiers.shift() {
                    shell.publish(ParamMessage::BeginSetParameter(self.param.as_ptr()));
                    state.drag_active = true;

                    // When holding down shift while clicking on a parameter we want to
                    // granuarly edit the parameter without jumping to a new value
                    let start_coord = match self.style {
                        SliderStyle::Horizontal => cursor_position.x,
                        SliderStyle::Vertical => cursor_position.y,
                        SliderStyle::Rotary(_) => {
                            // For rotary, store the Y coordinate for up/down dragging
                            cursor_position.y
                        }
                    };
                    state.granular_drag_start_x_value =
                        Some((start_coord, self.param.modulated_normalized_value()));
                } else {
                    shell.publish(ParamMessage::BeginSetParameter(self.param.as_ptr()));
                    state.drag_active = true;

                    match self.style {
                        SliderStyle::Rotary(_) => {
                            // For rotary sliders, don't jump to absolute position on click
                            // Instead, start relative dragging from current value
                            let start_coord = cursor_position.y;
                            state.granular_drag_start_x_value =
                                Some((start_coord, self.param.modulated_normalized_value()));
                        }
                        _ => {
                            // For horizontal and vertical sliders, use absolute positioning
                            self.set_normalized_value(
                                shell,
                                self.style.calculate_normalized_value(
                                    &bounds,
                                    cursor_position,
                                    self.param.default_normalized_value(),
                                ),
                            );
                            state.granular_drag_start_x_value = None;
                        }
                    }
                }

                event::Status::Captured
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Touch(touch::Event::FingerLifted { .. } | touch::Event::FingerLost { .. }) => {
                if !state.drag_active {
                    return;
                }

                shell.publish(ParamMessage::EndSetParameter(self.param.as_ptr()));
                state.drag_active = false;
                event::Status::Captured
            }
            Event::Mouse(mouse::Event::CursorMoved { .. })
            | Event::Touch(touch::Event::FingerMoved { .. }) => {
                // Don't do anything when we just reset the parameter because that would be weird
                if !state.drag_active {
                    return;
                }

                let bounds = layout.bounds();

                // If shift is being held then the drag should be more granular instead of
                // absolute
                if let Some(cursor_position) = cursor.position() {
                    match self.style {
                        SliderStyle::Rotary(_) => {
                            // For rotary sliders, always use relative/granular dragging
                            let current_coord = cursor_position.y;

                            // The granular_drag_start_x_value should always be set for rotary sliders
                            // during mouse button press, but we handle the fallback case here
                            let (drag_start_coord, drag_start_value) =
                                state.granular_drag_start_x_value.unwrap_or_else(|| {
                                    // Fallback: start from current position and value
                                    let current_value = self.param.modulated_normalized_value();
                                    state.granular_drag_start_x_value =
                                        Some((current_coord, current_value));
                                    (current_coord, current_value)
                                });

                            let multiplier = if state.keyboard_modifiers.shift() {
                                GRANULAR_DRAG_MULTIPLIER
                            } else {
                                GRANULAR_DRAG_MULTIPLIER * 10.0 // Normal drag is 10x faster than shift drag
                            };

                            let drag_delta = (drag_start_coord - current_coord) * multiplier; // Invert: up is negative Y change
                            let start_y = util::remap_rect_y_t(&bounds, 1.0 - drag_start_value);
                            let new_value =
                                1.0 - util::remap_rect_y_coordinate(&bounds, start_y - drag_delta); // Note: subtract delta because we inverted it

                            self.set_normalized_value(shell, new_value.clamp(0.0, 1.0));
                        }
                        _ => {
                            // For horizontal and vertical sliders, use original behavior
                            if state.keyboard_modifiers.shift() {
                                let current_coord = match self.style {
                                    SliderStyle::Horizontal => cursor_position.x,
                                    SliderStyle::Vertical => cursor_position.y,
                                    _ => unreachable!(),
                                };

                                let (drag_start_coord, drag_start_value) =
                                    *state.granular_drag_start_x_value.get_or_insert_with(|| {
                                        (current_coord, self.param.modulated_normalized_value())
                                    });

                                self.set_normalized_value(
                                    shell,
                                    self.style.calculate_granular_normalized_value(
                                        &bounds,
                                        drag_start_coord,
                                        current_coord,
                                        drag_start_value,
                                        self.param.default_normalized_value(),
                                    ),
                                );
                            } else {
                                state.granular_drag_start_x_value = None;

                                self.set_normalized_value(
                                    shell,
                                    self.style.calculate_normalized_value(
                                        &bounds,
                                        cursor_position,
                                        self.param.default_normalized_value(),
                                    ),
                                );
                            }
                        }
                    }
                }

                event::Status::Captured
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
                let bounds = layout.bounds();

                // Handle modifier changes during dragging
                if state.drag_active && state.granular_drag_start_x_value.is_some() {
                    match self.style {
                        SliderStyle::Rotary(_) => {
                            // For rotary sliders, when shift state changes, update the start position
                            // to current position to avoid jumps
                            if let Some(cursor_position) = cursor.position() {
                                let current_coord = cursor_position.y;
                                let current_value = self.param.modulated_normalized_value();
                                state.granular_drag_start_x_value =
                                    Some((current_coord, current_value));
                            }
                        }
                        _ => {
                            // For non-rotary sliders, if shift is released, snap to current cursor position
                            if !modifiers.shift() {
                                state.granular_drag_start_x_value = None;

                                if let Some(cursor_position) = cursor.position() {
                                    self.set_normalized_value(
                                        shell,
                                        self.style.calculate_normalized_value(
                                            &bounds,
                                            cursor_position,
                                            self.param.default_normalized_value(),
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }

                event::Status::Captured
            }
            _ => event::Status::Ignored,
        };
    }

    fn mouse_interaction(
        &self,
        _state: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, P> ParamSlider<'a, P>
where
    P: Param + 'a,
{
    /// Convert this [`ParamSlider`] into an [`Element`] with the correct message. You should have a
    /// variant on your own message type that wraps around [`ParamMessage`] so you can forward those
    /// messages to
    /// [`IcedEditor::handle_param_message()`][crate::IcedEditor::handle_param_message()].
    pub fn map<Message, Theme, Renderer, F>(self, f: F) -> Element<'a, Message, Theme, Renderer>
    where
        Message: 'static,
        F: Fn(ParamMessage) -> Message + 'static,
        Theme: text_input::Catalog + 'a,
        Renderer: TextRenderer + 'a,
        Renderer::Font: From<crate::Font>,
    {
        Element::from(self).map(f)
    }
}

impl<'a, P, Theme, Renderer> From<ParamSlider<'a, P>> for Element<'a, ParamMessage, Theme, Renderer>
where
    P: Param + 'a,
    Theme: text_input::Catalog + 'a,
    Renderer: TextRenderer + 'a,
    Renderer::Font: From<crate::Font>,
{
    fn from(widget: ParamSlider<'a, P>) -> Self {
        Element::new(widget)
    }
}
