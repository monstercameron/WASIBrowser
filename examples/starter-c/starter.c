/* starter-c: GoWebComponents shorthand for C (gwbc.h).
 * Declarative builders, prop options, hooks, conditional nodes, fn-based
 * lists, utility styling with hover, painless composition.
 *
 * Build: scripts\build-c.cmd examples\starter-c\starter.c renderer\starter-c.wasm
 */
#include "gwbc.h"

typedef struct {
    const char *title;
    i32 initialCount;
} StarterAppProps;

typedef struct {
    const char *name;
    i32 count;
    const char *previousCount;
    Handler onIncrement;
} CounterPanelProps;

/* Boring C render function — the GWC-style answer to inline closures. */
static Node renderDot(i32 i) {
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
            text("Previous count: %s", props.previousCount)),

        div(props(class(U(Flex, Gap(1)))),
            Range(minI32(props.count, 20), renderDot)),

        button(
            props(
                id("increment"),
                type("button"),
                onClick(props.onIncrement),
                class(U(RoundedXl, Px(4), Py(2), BgSlate900, FgWhite, TextSm,
                        Cursor("pointer"), Hover(BgSlate700)))
            ),
            "Increment"
        )
    );
}

component(StarterApp, props, StarterAppProps) {
    stateI32(count, props.initialCount);
    stateStr(name, "C developer");
    previousI32(previousCount, count);

    event(increment) {
        set(count, count + 1);
    }

    eventInput(updateName, e) {
        set(name, e.value);
    }

    char previousLabel[32] = "none yet";
    if (previousCount.ok) {
        fmtI32(previousLabel, previousCount.value);
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
                    onInput(updateName),
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

            CounterPanel(Props(CounterPanelProps,
                .name = name,
                .count = count,
                .previousCount = previousLabel,
                .onIncrement = increment,
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
    .initialCount = 0,
});
