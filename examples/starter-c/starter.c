/* starter-c: GoWebComponents shorthand for C (gwbc.h).
 * Declarative builders, prop options, hooks, conditional nodes, fn-based
 * lists, utility styling with hover, painless composition.
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

/* Boring C render function — the GWC-style answer to inline closures. */
static Node render_dot(i32 i) {
    (void)i;
    return span(props(class(U(TextSm, FgAmber500))), "*");
}

component(CounterPanel, props, CounterPanelProps) {
    return div(
        props(class(U(Flex, FlexCol, Gap(3), RoundedXl, BorderSlate200, BgWhite, Pad(5)))),

        p(props(class(U(TextSm, FgSlate600))),
            text("Hello, %s.", props.name)),

        p(props(class(U(TextLg, FontSemibold, FgSlate900))),
            text("Count: %d", props.count)),

        p(props(class(U(TextSm, FgSlate500))),
            text("Previous count: %s", props.previous_count)),

        div(props(class(U(Flex, Gap(1)))),
            Range(min_i32(props.count, 20), render_dot)),

        button(
            props(
                id("increment"),
                type("button"),
                on_click(props.on_increment),
                class(U(RoundedXl, Px(4), Py(2), BgSlate900, FgWhite, TextSm,
                        Cursor("pointer"), Hover(BgSlate700)))
            ),
            "Increment"
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

    return main(
        props(class(U(Block, BgSlate100, Px(6), Py(12), FgSlate900, RoundedXl, MaxW(120)))),

        div(
            props(class(U(Flex, FlexCol, Gap(6)))),

            h1(props(class(U(Text4xl, FontBold))), props.title),

            p(
                props(class(U(TextSm, FgSlate600))),
                "A starter that uses C components, markup helpers, utility styling, "
                "state, events, composition, and reactive text."
            ),

            input(
                props(
                    id("name-input"),
                    type("text"),
                    value(name),
                    on_input(update_name),
                    placeholder("Who is using the app?"),
                    class(U(WFull, RoundedXl, BorderSlate300, BgWhite, Px(4), Py(3), TextSm))
                )
            ),

            If(name[0] == 0,
                p(
                    props(class(U(TextSm, FgAmber500))),
                    "Tip: enter a name to personalize the panel."
                )
            ),

            CounterPanel(P_(CounterPanelProps,
                .name = name,
                .count = count,
                .previous_count = previous_label,
                .on_increment = increment,
            )),

            p(
                props(class(U(TextXs, FgSlate500))),
                text("Current count is %d", count)
            )
        )
    );
}

app(StarterApp, {
    .title = "GWB Starter",
    .initial_count = 0,
});
