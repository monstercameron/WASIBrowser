/* starter-c: React-like components in freestanding C (gwbc.h).
 * Lowercase host elements, PascalCase components, hooks, declarative
 * conditionals and lists, utility styling with hover, painless props.
 *
 * Build: scripts\build-c.cmd examples\starter-c\starter.c renderer\starter-c.wasm
 */
#include "gwbc.h"

typedef struct {
    const char *title;
    i32 initial_count;
} StarterAppProps;

typedef struct {
    const char *name;
    i32 count;
    const char *previous_count;
    Handler on_increment;
} CounterPanelProps;

component(CounterPanel, props, CounterPanelProps) {
    return view(
        div(
            class(Flex, FlexCol, Gap(3), RoundedXl, BorderSlate200, BgWhite, Pad(5)),

            p(class(TextSm, FgSlate600),
                text("Hello, %s.", props.name)),

            p(class(TextLg, FontSemibold, FgSlate900),
                text("Count: %d", props.count)),

            p(class(TextSm, FgSlate500),
                text("Previous count: %s", props.previous_count)),

            div(class(Flex, Gap(1)),
                map_range(i, min_i32(props.count, 20),
                    span(class(TextSm, FgAmber500), "*")
                )
            ),

            button(
                id("increment"),
                type("button"),
                on_click(props.on_increment),
                class(RoundedXl, Px(4), Py(2), BgSlate900, FgWhite, TextSm,
                      Cursor("pointer"), Hover(BgSlate700)),
                "Increment"
            )
        )
    );
}

component(StarterApp, props, StarterAppProps) {
    state_i32(count, props.initial_count);
    state_str(name, "C developer");
    previous_i32(previous_count, count);

    event(increment) {
        set(count, count + 1);
    }

    event_input(update_name, e) {
        set(name, e.value);
    }

    char previous_label[32] = "none yet";
    if (previous_count.ok) {
        fmt_i32(previous_label, previous_count.value);
    }

    return view(
        main(
            class(Block, BgSlate100, Px(6), Py(12), FgSlate900, RoundedXl, MaxW(120)),

            div(
                class(Flex, FlexCol, Gap(6)),

                h1(class(Text4xl, FontBold), props.title),

                p(
                    class(TextSm, FgSlate600),
                    "A starter that uses C components, macro shorthand tags, "
                    "utility styling, state, events, composition, and reactive text."
                ),

                input(
                    id("name-input"),
                    type("text"),
                    value(name),
                    on_input(update_name),
                    placeholder("Who is using the app?"),
                    class(WFull, RoundedXl, BorderSlate300, BgWhite, Px(4), Py(3), TextSm)
                ),

                show(name[0] == 0,
                    p(class(TextSm, FgAmber500),
                        "Tip: enter a name to personalize the panel.")
                ),

                child(CounterPanel, {
                    .name = name,
                    .count = count,
                    .previous_count = previous_label,
                    .on_increment = increment,
                }),

                p(class(TextXs, FgSlate500),
                    text("Current count is %d", count))
            )
        )
    );
}

app(StarterApp, {
    .title = "GWB Starter",
    .initial_count = 0,
});
