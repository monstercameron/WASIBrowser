/* starter-c: component-model demo — "JSX in C" via the preprocessor (gwbc.h).
 * State, events, composition, conditional + reactive text, utility styling.
 *
 * Build:
 *   clang --target=wasm32-unknown-unknown -O2 -nostdlib -fno-builtin \
 *         -Wl,--no-entry -Wl,--export-memory -I../../sdk-c \
 *         -o starter-c.wasm starter.c
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

COMPONENT(CounterPanel, CounterPanelProps, props) {
    RETURN(
        Div(
            U(Flex, FlexCol, Gap(3), RoundedXl, BorderSlate200, BgWhite, Pad(5)),

            P(
                U(TextSm, FgSlate600),
                T("Hello, %s.", PROP(name))
            ),

            P(
                U(TextLg, FontSemibold, FgSlate900),
                T("Count: %d", PROP(count))
            ),

            P(
                U(TextSm, FgSlate500),
                T("Previous count: %s", PROP(previous_count))
            ),

            Button(
                Type("button"),
                OnClick(PROP(on_increment)),
                U(RoundedXl, Px(4), Py(2), BgSlate900, FgWhite, TextSm, Cursor("pointer")),
                "Increment"
            )
        )
    );
}

COMPONENT(StarterApp, StarterAppProps, props) {
    STATE(i32, count, PROP(initial_count));
    STATE_STR(name, "C developer");
    PREVIOUS(i32, previous_count, count);

    EVENT(increment) {
        SET(count, count + 1);
    }

    EVENT_INPUT(update_name, e) {
        SET(name, e.value);
    }

    char previous_label[32] = "none yet";

    IF(previous_count.ok) {
        fmt_i32(previous_label, previous_count.value);
    }

    RETURN(
        Main(
            U(Block, BgSlate100, Px(6), Py(12), FgSlate900, RoundedXl, MaxW(120)),

            Div(
                U(Flex, FlexCol, Gap(6)),

                H1(
                    U(Text4xl, FontBold),
                    PROP(title)
                ),

                P(
                    U(TextSm, FgSlate600),
                    "A starter that uses C components, macro shorthand tags, "
                    "utility styling, state, events, composition, and reactive text."
                ),

                Input(
                    Type("text"),
                    Value(name),
                    OnInput(update_name),
                    Placeholder("Who is using the app?"),
                    U(WFull, RoundedXl, BorderSlate300, BgWhite, Px(4), Py(3), TextSm)
                ),

                WHEN(
                    name[0] == 0,
                    P(
                        U(TextSm, FgAmber500),
                        "Tip: enter a name to personalize the panel."
                    )
                ),

                USE(
                    CounterPanel,
                    PROPS(CounterPanelProps,
                        .name = name,
                        .count = count,
                        .previous_count = previous_label,
                        .on_increment = increment
                    )
                ),

                P(
                    U(TextXs, FgSlate500),
                    T("Current count is %d", count)
                )
            )
        )
    );
}

GWB_APP(StarterApp, PROPS(StarterAppProps, .title = "GWB Starter", .initial_count = 0))
