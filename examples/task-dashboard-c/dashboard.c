/* dashboard.c — larger component-model demo for gwbc.h.
 *
 * Demonstrates: nested components, typed props, prop drilling, context,
 * struct/enum/str state, previous state, payload-bound handlers (bindI32),
 * imported plain-C business logic, keyed+filtered lists (mapKeyedIf),
 * conditional rendering, reusable layout components with children-as-props,
 * disabled buttons, event logging, and a debug panel.
 *
 * Build:
 *   clang --target=wasm32-unknown-unknown -O2 -nostdlib -fno-builtin
 *         -Wl,--no-entry -Wl,--export-memory -I../../sdk-c
 *         -o dashboard-c.wasm dashboard.c business.c
 */
#include "gwbc.h"
#include "gwbc-tw.h" /* typed Tailwind utilities + Preflight base css */
#include "business.h"

/* --------------------------------------------------------------- app css
 * The raw-CSS lane (appCss + cls): keyframe animations, descendant
 * selectors, and gradients — everything the utility tokens can't say. */
static const char *dashCss =
    "@keyframes fadeUp { from { opacity: 0; transform: translateY(10px); }"
    "  to { opacity: 1; transform: translateY(0); } }"
    "@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: .45; } }"
    "@keyframes spin { from { transform: rotate(0deg); }"
    "  to { transform: rotate(360deg); } }"
    "#mount .anim-in { animation: fadeUp .4s ease-out; }"
    "#mount .pulse { animation: pulse 1.6s ease-in-out infinite; }"
    "#mount .spinner { width: 14px; height: 14px; flex: 0 0 auto;"
    "  border: 2px solid #cbd5e1; border-top-color: #0f172a;"
    "  border-radius: 9999px; animation: spin .8s linear infinite; }"
    "#mount .page-bg { background:"
    "  linear-gradient(180deg, #f8fafc 0%, #e7edf6 100%); }"
    "#mount .lift { transition: box-shadow .2s ease, transform .2s ease,"
    "  border-color .2s ease; }"
    "#mount .lift:hover { transform: translateY(-2px);"
    "  box-shadow: 0 12px 28px rgba(15, 23, 42, .14); }"
    "#mount .btn-primary { background:"
    "  linear-gradient(135deg, #1e293b 0%, #38506f 100%); }"
    "#mount .btn-primary:hover { background:"
    "  linear-gradient(135deg, #0f172a 0%, #1e293b 100%); }"
    "#mount .task-row .row-actions { opacity: .35;"
    "  transition: opacity .18s ease; }"
    "#mount .task-row:hover .row-actions { opacity: 1; }"
    "#mount .bar-track { height: 8px; border-radius: 9999px;"
    "  background: #e2e8f0; overflow: hidden; }"
    "#mount .bar-fill { height: 100%; border-radius: 9999px;"
    "  background: linear-gradient(90deg, #34d399, #10b981);"
    "  transition: width .4s ease; }"
    "#mount .seg { transition: background-color .15s ease, color .15s ease; }"
    /* dark-mode variants for the raw-CSS pieces (root gets .dark) */
    "#mount .page-bg-dark { background:"
    "  linear-gradient(180deg, #0b1220 0%, #131c2e 100%); }"
    "#mount .dark .bar-track { background: #1f2a3d; }"
    "#mount .dark .spinner { border-color: #334155;"
    "  border-top-color: #e2e8f0; }"
    "#mount .dark .lift:hover { box-shadow: 0 12px 28px rgba(0, 0, 0, .5); }";

/* ---------------------------------------------------------------- context */

typedef struct {
    const char *appName;
    const char *accentName;
    i32 compact;
    i32 dark; /* toolbar dark-mode toggle -> THEME_CHANGE event -> here */
} ThemeValue;

context(ThemeContext, ThemeValue);

/* Shared global state: bumped by app-level events, read directly by
 * DebugPanel — no prop drilling. */
atomI32(interactionCount, 0);

/* ------------------------------------------------------- primitive pieces */

typedef struct {
    const char *label; /* must outlive the render: literal or state string */
    const char *tone;  /* "primary" | "danger" | "plain" */
    const char *domId; /* optional */
    Handler onPress;
    i32 isDisabled;
} AppButtonProps;

component(AppButton, props, AppButtonProps) {
    ThemeValue theme = useContext(ThemeContext);

    return button(
        If(props.domId != 0, id(props.domId)),
        type("button"),
        disabled(props.isDisabled),
        onClick(props.onPress),
        class(U(RoundedXl, Px(4), Py(2), TextSm, Cursor("pointer"), TwTransition)),
        classIf(strEq(props.tone, "primary"), cls("btn-primary"), FgWhite),
        classIf(strEq(props.tone, "danger"), BgRed600, FgWhite, Hover(BgRed700)),
        classIf(strEq(props.tone, "plain") && !theme.dark,
            BgWhite, FgSlate900, BorderSlate300, Hover(BgSlate100)),
        classIf(strEq(props.tone, "plain") && theme.dark,
            twBg(TwSlate, 800), twTextColor(TwSlate, 100), twBorder(1),
            twBorderColor(TwSlate, 600), Hover(twBg(TwSlate, 700))),
        classIf(props.isDisabled, Opacity60),
        classIf(theme.compact, Px(3), Py(1)),
        props.label
    );
}

typedef struct {
    const char *label;
    i32 amount; /* numbers as i32 props: the child formats via Text() */
    i32 warn;
    TwColor tone; /* left accent bar hue */
} StatPillProps;

/* One <dt>/<dd> pair per stat; the parent StatsGrid provides the <dl>.
 * The accent is an inner strip div — mixed per-side border widths don't
 * paint in the current engine build (uniform borders only). */
component(StatPill, props, StatPillProps) {
    ThemeValue theme = useContext(ThemeContext);

    return div(
        cls("lift anim-in"),
        class(U(twRounded(TwRoundedXl), twBorder(1), twPx(4), twPy(3),
                TwFlex, twGap(3), twShadow(TwShadowSm))),
        classIf(!theme.dark, TwBgWhite, twBorderColor(TwSlate, 200)),
        classIf(theme.dark, twBg(TwSlate, 800), twBorderColor(TwSlate, 700)),
        div(class(U(twW(1), twRounded(TwRoundedFull), twBg(props.tone, 400)))),
        div(
            class(U(TwFlex, TwFlexCol, twGap(1))),
            dt(class(U(twTextSize(TwTextXs), TwUppercase, TwTrackingWide)),
                classIf(!theme.dark, twTextColor(TwSlate, 500)),
                classIf(theme.dark, twTextColor(TwSlate, 400)),
                props.label),
            dd(strong(
                class(U(twTextSize(TwText2xl), FontSemibold)),
                classIf(props.warn, twTextColor(TwAmber, 500), cls("pulse")),
                classIf(!props.warn && !theme.dark, twTextColor(TwSlate, 900)),
                classIf(!props.warn && theme.dark, twTextColor(TwSlate, 100)),
                Text(props.amount)
            ))
        )
    );
}

/* ---------------------------------------------------------- layout pieces */

typedef struct {
    const char *title;
    const char *subtitle;
    const char *appName;
} PageHeaderProps;

component(PageHeader, props, PageHeaderProps) {
    ThemeValue theme = useContext(ThemeContext);

    return header(
        cls("anim-in"),
        class(U(Flex, FlexCol, Gap(2))),
        span(class(U(twTextSize(TwTextXs), TwUppercase, TwTrackingWide,
                     twTextColor(TwSlate, theme.dark ? 400 : 500))),
            props.appName),
        h1(class(U(Text4xl, FontBold)),
            classIf(!theme.dark, FgSlate900),
            classIf(theme.dark, twTextColor(TwSlate, 100)),
            props.title),
        p(class(U(TextSm)),
            classIf(!theme.dark, FgSlate600),
            classIf(theme.dark, twTextColor(TwSlate, 400)),
            props.subtitle)
    );
}

typedef struct {
    const char *title;
    const char *subtitle;
    Children children;
} PageProps;

component(Page, props, PageProps) {
    ThemeValue theme = useContext(ThemeContext);

    return main(
        If(theme.dark, cls("dark")), /* raw-CSS descendant hook */
        IfElse(theme.dark, cls("page-bg-dark"), cls("page-bg")),
        class(U(TwBlock, twPx(6), twPy(8))),
        classIf(!theme.dark, twTextColor(TwSlate, 900)),
        classIf(theme.dark, twTextColor(TwSlate, 100)),
        div(
            class(U(MaxW(200), MxAuto, Flex, FlexCol, Gap(5))),
            PageHeader(Props(PageHeaderProps,
                .title = props.title,
                .subtitle = props.subtitle,
                .appName = theme.appName,
            )),
            props.children,
            footer(
                class(U(twBorderT(1), twPt(4),
                        TwFlex, TwItemsCenter, TwJustifyBetween,
                        twTextSize(TwTextXs), twTextColor(TwSlate, 500))),
                classIf(!theme.dark, twBorderColor(TwSlate, 200)),
                classIf(theme.dark, twBorderColor(TwSlate, 700)),
                small(abbr("GWB"), " - a wasm-first browser platform. Zero JavaScript."),
                small("Guest: ", code(class(U(TwFontMono)), "dashboard.c"),
                    " ", time("2026"))
            )
        )
    );
}

typedef struct {
    const char *title;
    Children children;
    Node rootExtra; /* additive prop bundle: propGroup(...) — zero = none */
} CardProps;

component(Card, props, CardProps) {
    ThemeValue theme = useContext(ThemeContext);

    return section(
        cls("lift anim-in"),
        class(U(twRounded(TwRounded2xl), twBorder(1), Pad(5),
                Flex, FlexCol, Gap(4), twShadow(TwShadowSm))),
        classIf(!theme.dark, BgWhite, twBorderColor(TwSlate, 200)),
        classIf(theme.dark, twBg(TwSlate, 800), twBorderColor(TwSlate, 700)),
        props.rootExtra,
        If(props.title && props.title[0],
            h2(class(U(TextLg, FontSemibold)),
                classIf(!theme.dark, FgSlate900),
                classIf(theme.dark, twTextColor(TwSlate, 100)),
                props.title)),
        props.children
    );
}

/* ------------------------------------------------------------- stats grid */

typedef struct {
    TaskStats stats;
} StatsGridProps;

component(StatsGrid, props, StatsGridProps) {
    i32 pct = props.stats.total
        ? props.stats.done * 100 / props.stats.total
        : 0;

    return div(
        class(U(Flex, FlexCol, Gap(3))),
        dl(
            class(U(Grid, GridCols4, Gap(3))),
            StatPill(Props(StatPillProps, .label = "Total",
                .amount = props.stats.total, .tone = TwSky)),
            StatPill(Props(StatPillProps, .label = "Open",
                .amount = props.stats.open, .tone = TwIndigo)),
            StatPill(Props(StatPillProps, .label = "Done",
                .amount = props.stats.done, .tone = TwEmerald)),
            StatPill(Props(StatPillProps,
                .label = "High priority",
                .amount = props.stats.highPriorityOpen,
                .warn = props.stats.highPriorityOpen > 0,
                .tone = TwAmber,
            ))
        ),
        /* animated completion bar: inline width transitions via .bar-fill */
        div(
            cls("anim-in"),
            class(U(Flex, FlexCol, Gap(1))),
            div(
                class(U(Flex, JustifyBetween)),
                span(class(U(twTextSize(TwTextXs), twTextColor(TwSlate, 500))),
                    "Completion"),
                span(id("completion-label"),
                    class(U(twTextSize(TwTextXs), twTextColor(TwSlate, 600),
                            FontSemibold)),
                    text("%d%%", pct))
            ),
            div(cls("bar-track"),
                div(id("completion-fill"), cls("bar-fill"),
                    css(gc_style(GWB_STYLE_WIDTH, strf("%d%%", pct)))))
        )
    );
}

/* ----------------------------------------------------------- task composer */

typedef struct {
    const char *draftTitle;
    i32 priority;
    Handler onTitleInput;
    Handler onTitleKey;   /* Enter submits */
    Handler onTitleFocus;
    Handler onTitleBlur;
    Handler onPriorityCycle;
    Handler onPriorityWheel; /* wheel over the readout adjusts 1..3 */
    Handler onSubmit;
} TaskComposerProps;

component(TaskComposer, props, TaskComposerProps) {
    ThemeValue theme = useContext(ThemeContext);

    return Card(Props(CardProps,
        .title = "Add task",
        .children = Children(
            form(
                class(U(Flex, FlexCol, Gap(3))),
                div(
                    class(U(Flex, Gap(2), TwItemsEnd)),
                    label(
                        class(U(WFull, Flex, FlexCol, Gap(1))),
                        span(class(U(twTextSize(TwTextXs), twTextColor(TwSlate, 500))),
                            "Title"),
                        input(
                            keyed("draft-input"), /* host node survives re-renders:
                                                     focus + caret genuinely preserved */
                            id("task-title"),
                            type("text"),
                            value(props.draftTitle),
                            onInput(props.onTitleInput),
                            onKeyDown(props.onTitleKey),
                            onFocus(props.onTitleFocus),
                            onBlur(props.onTitleBlur),
                            placeholder("What needs doing?"),
                            class(U(WFull, RoundedXl, Px(4), Py(3), TextSm)),
                            classIf(!theme.dark, BorderSlate300, BgWhite),
                            classIf(theme.dark, twBorder(1),
                                twBorderColor(TwSlate, 600), twBg(TwSlate, 900),
                                twTextColor(TwSlate, 100))
                        )
                    ),
                    AppButton(Props(AppButtonProps,
                        .label = "Add",
                        .tone = "primary",
                        .domId = "add-task",
                        .onPress = props.onSubmit,
                        .isDisabled = props.draftTitle[0] == 0,
                    ))
                ),
                div(
                    class(U(Flex, Gap(2), ItemsCenter)),
                    output(
                        id("priority-output"),
                        onWheel(props.onPriorityWheel),
                        class(U(TextSm, FgSlate600, TwSelectNone)),
                        text("Priority: %d", props.priority)),
                    AppButton(Props(AppButtonProps,
                        .label = "Cycle priority",
                        .tone = "plain",
                        .domId = "cycle-priority",
                        .onPress = props.onPriorityCycle,
                    )),
                    small(class(U(twTextSize(TwTextXs), twTextColor(TwSlate, 400))),
                        "scale ", kbd("1"), "-", kbd("3"))
                )
            )
        ),
    ));
}

/* ---------------------------------------------------------------- toolbar */

typedef struct {
    const char *label;
    const char *domId;
    i32 active;
    Handler onPress;
} FilterSegProps;

/* One segment of the filter control: pill-in-a-track, active state lifted. */
component(FilterSeg, props, FilterSegProps) {
    ThemeValue theme = useContext(ThemeContext);

    return button(
        id(props.domId),
        type("button"),
        onClick(props.onPress),
        cls("seg"),
        class(U(TextSm, twPx(3), twPy(1), twRounded(TwRoundedLg),
                Cursor("pointer"))),
        classIf(props.active && !theme.dark, TwBgWhite,
            twTextColor(TwSlate, 900), FontSemibold, twShadow(TwShadowSm)),
        classIf(props.active && theme.dark, twBg(TwSlate, 600),
            twTextColor(TwSlate, 100), FontSemibold, twShadow(TwShadowSm)),
        classIf(!props.active && !theme.dark, twTextColor(TwSlate, 500),
            Hover(BgSlate100)),
        classIf(!props.active && theme.dark, twTextColor(TwSlate, 400),
            Hover(twBg(TwSlate, 700))),
        props.label
    );
}

typedef struct {
    TaskFilter filter;
    Handler onSetFilter; /* eventI32: payload is the TaskFilter to select */
    Handler onClearDone;
    i32 hasDone;
} ToolbarProps;

component(Toolbar, props, ToolbarProps) {
    ThemeValue theme = useContext(ThemeContext);

    return nav(
        cls("anim-in"),
        class(U(Flex, Gap(2), ItemsCenter, JustifyBetween)),
        /* segmented control beats a blind cycle button: one click to any
         * view, and the active state is visible */
        div(
            class(U(Flex, Gap(1), ItemsCenter, twP(1),
                    twRounded(TwRoundedXl))),
            classIf(!theme.dark, twBg(TwSlate, 200)),
            classIf(theme.dark, twBg(TwSlate, 800)),
            FilterSeg(Props(FilterSegProps, .label = "All",
                .domId = "filter-all",
                .active = props.filter == FilterAll,
                .onPress = bindI32(props.onSetFilter, (i32)FilterAll))),
            FilterSeg(Props(FilterSegProps, .label = "Open",
                .domId = "filter-open",
                .active = props.filter == FilterOpen,
                .onPress = bindI32(props.onSetFilter, (i32)FilterOpen))),
            FilterSeg(Props(FilterSegProps, .label = "Done",
                .domId = "filter-done",
                .active = props.filter == FilterDone,
                .onPress = bindI32(props.onSetFilter, (i32)FilterDone)))
        ),
        AppButton(Props(AppButtonProps,
            .label = "Clear done",
            .tone = "danger",
            .domId = "clear-done",
            .onPress = props.onClearDone,
            .isDisabled = !props.hasDone,
        ))
    );
}

/* --------------------------------------------------------------- task row */

typedef struct {
    const Task *task; /* points into the persistent store: outlives the render */
    i32 isHovered;
    Handler onToggle;
    Handler onRemove;
    Handler onHoverEnter;
    Handler onHoverLeave;
    Handler onContext;
} TaskRowProps;

component(TaskRow, props, TaskRowProps) {
    ThemeValue theme = useContext(ThemeContext);
    const Task *task = props.task;

    return li(
        id(strf("task-row-%d", task->id)),
        onHover(props.onHoverEnter, props.onHoverLeave),
        onDblClick(props.onToggle),      /* double-click anywhere toggles */
        onContextMenu(props.onContext),  /* right-click marks (Prevent'd) */
        cls("task-row anim-in"),         /* actions fade in on row hover */
        class(U(Flex, ItemsCenter, JustifyBetween, Gap(3), RoundedXl,
                twBorder(1), Px(4), Py(3), TwTransition)),
        classIf(!theme.dark, BorderSlate200, BgWhite),
        classIf(theme.dark, twBorderColor(TwSlate, 700), twBg(TwSlate, 800)),
        classIf(props.isHovered && !theme.dark,
            twBg(TwSlate, 50), twBorderColor(TwSlate, 300)),
        classIf(props.isHovered && theme.dark,
            twBg(TwSlate, 700), twBorderColor(TwSlate, 500)),
        classIf(task->done, Opacity60),
        div(
            class(U(Flex, FlexCol, Gap(1))),
            /* done tasks strike through via the <s> element (UA styling),
             * not a utility class */
            IfElse(task->done,
                s(class(U(TextSm, FontSemibold)),
                    classIf(!theme.dark, FgSlate900),
                    classIf(theme.dark, twTextColor(TwSlate, 100)),
                    task->title),
                span(class(U(TextSm, FontSemibold)),
                    classIf(!theme.dark, FgSlate900),
                    classIf(theme.dark, twTextColor(TwSlate, 100)),
                    task->title)),
            small(class(U(twTextSize(TwTextXs), twTextColor(TwSlate, theme.dark ? 400 : 500))),
                text("Priority %d - %s theme", task->priority, theme.accentName))
        ),
        div(
            cls("row-actions"),
            class(U(Flex, Gap(2))),
            AppButton(Props(AppButtonProps,
                .label = task->done ? "Reopen" : "Done",
                .tone = "plain",
                .domId = strf("task-toggle-%d", task->id),
                .onPress = props.onToggle,
            )),
            AppButton(Props(AppButtonProps,
                .label = "Remove",
                .tone = "danger",
                .domId = strf("task-remove-%d", task->id),
                .onPress = props.onRemove,
            ))
        )
    );
}

/* -------------------------------------------------------------- task list */

typedef struct {
    const TaskStore *store; /* immutable-ish input state */
    TaskFilter filter;      /* view state */
    i32 hoveredTaskId;
    Handler onToggleTask;   /* behavior */
    Handler onRemoveTask;
    Handler onHoverTask;
    Handler onUnhoverTask;
    Handler onContextTask;
} TaskListProps;

component(TaskList, props, TaskListProps) {
    i32 visible = TaskStore_CountMatching(props.store, props.filter);

    return Card(Props(CardProps,
        .title = "Tasks",
        .children = Children(
            IfElse(visible == 0,
                p(class(U(TextSm, FgSlate500)),
                    IfElse(props.store->count == 0,
                        Text("No tasks yet."),
                        text("No %s tasks.", Filter_Label(props.filter)))),
                ul(
                    class(U(Flex, FlexCol, Gap(2))),
                    mapKeyedIf(task, props.store->items, props.store->count,
                        task->id,
                        Task_MatchesFilter(task, props.filter),
                        TaskRow(Props(TaskRowProps,
                            .task = task,
                            .isHovered = props.hoveredTaskId == task->id,
                            .onToggle = bindI32(props.onToggleTask, task->id),
                            .onRemove = bindI32(props.onRemoveTask, task->id),
                            .onHoverEnter = bindI32(props.onHoverTask, task->id),
                            .onHoverLeave = props.onUnhoverTask,
                            .onContext = bindI32(props.onContextTask, task->id),
                        ))))
            )
        ),
    ));
}

/* ------------------------------------------------------------ remote data */

/* Demo-grade JSON scraping: pull "title" values out of the jsonplaceholder
 * todos payload. A real app would ship a proper parser as business code. */
static char remoteTitles[8][96];
static i32 remoteTitleCount;

static void parseRemoteTitles(const char *json) {
    remoteTitleCount = 0;
    const char *p = json;
    while (remoteTitleCount < 8) {
        const char *needle = "\"title\": \"";
        const char *hit = 0;
        for (const char *s = p; *s; s++) {
            const char *a = s, *b = needle;
            while (*a && *b && *a == *b) { a++; b++; }
            if (!*b) { hit = a; break; }
        }
        if (!hit) break;
        i32 n = 0;
        while (hit[n] && hit[n] != '"' && n < 95) {
            remoteTitles[remoteTitleCount][n] = hit[n];
            n++;
        }
        remoteTitles[remoteTitleCount][n] = 0;
        remoteTitleCount++;
        p = hit + n;
    }
}

component0(RemoteTodos) {
    ThemeValue theme = useContext(ThemeContext);
    QueryResult q = useQuery("remote-todos",
        "https://jsonplaceholder.typicode.com/todos?_limit=5");

    event(refetchRemote) {
        logf("[dashboard] refetching remote todos");
        refetchQuery("remote-todos");
    }

    if (q.ok) parseRemoteTitles(q.data);

    return Card(Props(CardProps,
        .title = "Remote todos (useQuery)",
        .children = Children(
            IfElse(q.loading,
                div(class(U(Flex, Gap(2), ItemsCenter)),
                    span(cls("spinner")),
                    p(class(U(TextSm, FgSlate500)), "Loading remote todos...")),
                IfElse(q.err,
                    p(class(U(TextSm, FgAmber500)),
                        text("Fetch failed (HTTP %d): %s", (i32)q.httpStatus, q.data)),
                    ol(
                        class(U(Flex, FlexCol, Gap(1))),
                        map(t, remoteTitles, remoteTitleCount,
                            li(id("remote-row"),
                                class(U(TextSm, twTextColor(TwSlate,
                                    theme.dark ? 400 : 600))),
                                text("- %s", *t)))
                    ))),
            div(
                class(U(Flex, Gap(2), ItemsCenter)),
                AppButton(Props(AppButtonProps,
                    .label = "Refetch",
                    .tone = "plain",
                    .domId = "refetch-remote",
                    .onPress = refetchRemote,
                    .isDisabled = q.fetching,
                )),
                /* stale-while-revalidate: old data stays; just hint */
                If(q.fetching && !q.loading, span(cls("spinner"))),
                If(q.fetching && !q.loading,
                    em(class(U(TextXs, FgSlate500)), "refreshing..."))
            )
        ),
    ));
}

/* ------------------------------------------------------------- debug panel */

typedef struct {
    i32 renderCount;
    i32 lastChangedTaskId;
    i32 hoveredTaskId;
    i32 editingDraft;
    i32 vpW, vpH;
    TaskFilter previousFilter;
    TaskFilter currentFilter;
} DebugPanelProps;

component(DebugPanel, props, DebugPanelProps) {
    ThemeValue theme = useContext(ThemeContext);
    i32 interactions = useAtom(interactionCount); /* atom read: no props */

    return Card(Props(CardProps,
        .title = "Debug",
        .rootExtra = theme.dark ? propGroup(twBg(TwSlate, 900))
                                : propGroup(BgSlate50),
        .children = Children(
            dl(
                class(U(Grid, GridCols2, Gap(1), TextXs,
                        twTextColor(TwSlate, theme.dark ? 400 : 500))),
                dt("Render count"), dd(Text(props.renderCount)),
                dt("Interactions"), dd(Text(interactions)),
                dt("Last changed task id"), dd(Text(props.lastChangedTaskId)),
                dt("Hovered task id"), dd(Text(props.hoveredTaskId)),
                dt("Editing draft"),
                dd(IfElse(props.editingDraft, Text("yes"), Text("no"))),
                dt("Viewport"), dd(text("%dx%d", props.vpW, props.vpH)),
                dt("Filter change"),
                dd(text("%s -> %s",
                    Filter_Label(props.previousFilter),
                    Filter_Label(props.currentFilter)))
            )
        ),
    ));
}

/* ------------------------------------------------------------------- app */

typedef struct {
    const char *title;
} DashboardAppProps;

/* ---- effects: run AFTER commit, never during render ---- */

static void onDashboardMount(void *ud) {
    (void)ud;
    logf("[effect] dashboard mounted (committed renders: %d)", renderCount());
}

typedef struct {
    TaskFilter filter;
} FilterFxCtx;

static void onFilterApplied(void *ud) {
    FilterFxCtx *c = ud;
    logf("[effect] filter effect: now syncing \"%s\"", Filter_Label(c->filter));
}

static void onFilterLeave(void *ud) {
    FilterFxCtx *c = ud; /* cleanup sees the OLD context */
    logf("[effect] filter cleanup: leaving \"%s\"", Filter_Label(c->filter));
}

typedef struct {
    TaskStats stats;
} StatsFxCtx;

static void onStatsChanged(void *ud) {
    StatsFxCtx *c = ud;
    logf("[effect] stats: %d total / %d open / %d done / %d high-priority (%d%% complete)",
        c->stats.total, c->stats.open, c->stats.done, c->stats.highPriorityOpen,
        c->stats.total ? c->stats.done * 100 / c->stats.total : 0);
}

component(DashboardApp, props, DashboardAppProps) {
    appCss(dashCss); /* raw-CSS lane: keyframes, gradients, hover reveals */

    stateStruct(TaskStore, store, TaskStore_Init);
    stateStr(draftTitle, "");
    stateI32(draftPriority, 2);
    stateEnum(TaskFilter, filter, FilterAll);
    stateI32(lastChangedTaskId, 0);
    stateI32(hoveredTaskId, 0);
    stateBool(editingDraft, 0);
    stateBool(darkMode, 0);
    stateI32(vpW, 0);
    stateI32(vpH, 0);
    previousI32(previousFilter, filter);

    /* Derived business data (recomputed each render; events ran already). */
    TaskStats stats = TaskStore_Stats(store);

    /* Effects: recorded during the commit pass, executed after the tree is
     * applied. Mount-once, and re-run on filter change (cleanup gets the
     * previous filter's context). */
    useEffect("dash-mount", onDashboardMount, 0, deps0());
    useEffectCtx("filter-sync", onFilterApplied, onFilterLeave,
        ((FilterFxCtx){ .filter = filter }), depsI32((i32)filter));
    /* re-logs whenever the store's shape actually changes — inspection lane */
    useEffectCtx("stats-sync", onStatsChanged, 0,
        ((StatsFxCtx){ .stats = stats }),
        deps2(stats.total, stats.done * 16 + stats.highPriorityOpen));

    eventInput(updateDraftTitle, e) {
        logf("[dashboard] draft title -> \"%s\"", e.value);
        set(draftTitle, e.value);
    }

    /* ---- browser interaction events beyond click/input ---- */

    eventResize(pageLoaded, v) { /* onLoad: fires once after initial mount */
        logf("[dashboard] page load: %dx%d", (i32)v.w, (i32)v.h);
        set(vpW, (i32)v.w);
        set(vpH, (i32)v.h);
    }

    eventResize(viewportResized, v) {
        logf("[dashboard] window resize: %dx%d", (i32)v.w, (i32)v.h);
        set(vpW, (i32)v.w);
        set(vpH, (i32)v.h);
    }

    eventTheme(themeChanged, t) { /* toolbar toggle or OS theme switch */
        logf("[dashboard] theme -> %s", t.dark ? "dark" : "light");
        set(darkMode, t.dark);
    }

    eventKey(draftKey, k) { /* Enter in the title input submits */
        logf("[dashboard] keydown \"%s\" in draft (mods=%d)", k.key, k.mods);
        if (strEq(k.key, "Enter") && draftTitle[0]) {
            setAtom(interactionCount, useAtom(interactionCount) + 1);
            if (TaskStore_Add(store, draftTitle, draftPriority)) {
                logf("[dashboard] added task #%d \"%s\" via Enter",
                    store->nextId - 1, draftTitle);
                set(draftTitle, "");
                set(draftPriority, 2);
            }
        }
    }

    event(draftFocused) { logf("[dashboard] draft input focused"); set(editingDraft, 1); }
    event(draftBlurred) { logf("[dashboard] draft input blurred"); set(editingDraft, 0); }

    eventWheel(priorityWheel, w) { /* wheel over the priority readout */
        i32 next = w.dy < 0
            ? (draftPriority >= 3 ? 3 : draftPriority + 1)
            : (draftPriority <= 1 ? 1 : draftPriority - 1);
        if (next != draftPriority) {
            logf("[dashboard] priority wheel %s -> %d", w.dy < 0 ? "up" : "down", next);
            set(draftPriority, next);
        }
    }

    eventI32(hoverTask, taskId) {
        logf("[dashboard] hover task #%d", taskId);
        set(hoveredTaskId, taskId);
    }
    event(unhoverTask) {
        logf("[dashboard] hover cleared (was #%d)", hoveredTaskId);
        set(hoveredTaskId, 0);
    }

    eventI32(contextTask, taskId) { /* right-click; default menu suppressed */
        logf("[dashboard] context menu on task #%d (default suppressed)", taskId);
        set(lastChangedTaskId, taskId);
    }

    event(cyclePriority) {
        i32 next = draftPriority >= 3 ? 1 : draftPriority + 1;
        logf("[dashboard] priority %d -> %d", draftPriority, next);
        setAtom(interactionCount, useAtom(interactionCount) + 1);
        set(draftPriority, next);
    }

    event(addTask) {
        setAtom(interactionCount, useAtom(interactionCount) + 1);
        if (TaskStore_Add(store, draftTitle, draftPriority)) {
            logf("[dashboard] added task #%d \"%s\" (priority %d), %d total",
                store->nextId - 1, draftTitle, draftPriority, store->count);
            set(draftTitle, "");
            set(draftPriority, 2);
        } else {
            logWarnf("[dashboard] add rejected (empty title or store full)");
        }
    }

    eventI32(setFilter, which) {
        logf("[dashboard] filter %s -> %s (segmented)",
            Filter_Label(filter), Filter_Label((TaskFilter)which));
        setAtom(interactionCount, useAtom(interactionCount) + 1);
        set(filter, (TaskFilter)which);
    }

    event(clearDone) {
        i32 before = store->count;
        TaskStore_ClearDone(store);
        setAtom(interactionCount, useAtom(interactionCount) + 1);
        logf("[dashboard] cleared %d done task(s), %d remain",
            before - store->count, store->count);
    }

    eventI32(toggleTask, taskId) {
        if (TaskStore_Toggle(store, taskId)) {
            logf("[dashboard] toggled task #%d", taskId);
            setAtom(interactionCount, useAtom(interactionCount) + 1);
            set(lastChangedTaskId, taskId);
        }
    }

    eventI32(removeTask, taskId) {
        if (TaskStore_Remove(store, taskId)) {
            logf("[dashboard] removed task #%d, %d remain", taskId, store->count);
            setAtom(interactionCount, useAtom(interactionCount) + 1);
            set(lastChangedTaskId, taskId);
        }
    }

    /* useMemo: recomputes ONLY when the task total changes (filter clicks,
     * toggles, typing leave it cached). gSummaryComputes proves it. */
    static i32 gSummaryComputes;
    const char *summary = memoStr("storeSummary", depsI32(stats.total),
        (gSummaryComputes++,
         strf("%d task(s) tracked - summary computed %dx", stats.total, gSummaryComputes)));

    ThemeValue theme = {
        .appName = "GoWebComponents C",
        .accentName = darkMode ? "midnight" : "slate",
        .compact = stats.total > 8,
        .dark = darkMode,
    };

    return provider(ThemeContext, theme,
        Page(Props(PageProps,
            .title = props.title,
            .subtitle = "State, context, prop drilling, payload handlers, "
                        "keyed lists, and imported business logic - in C.",
            .children = Children(
                /* window-level hooks: root-subscribed regardless of position */
                onLoad(pageLoaded),
                onWindowResize(viewportResized),
                onThemeChange(themeChanged),

                StatsGrid(Props(StatsGridProps, .stats = stats)),

                TaskComposer(Props(TaskComposerProps,
                    .draftTitle = draftTitle,
                    .priority = draftPriority,
                    .onTitleInput = updateDraftTitle,
                    .onTitleKey = draftKey,
                    .onTitleFocus = draftFocused,
                    .onTitleBlur = draftBlurred,
                    .onPriorityCycle = cyclePriority,
                    .onPriorityWheel = Prevent(priorityWheel),
                    .onSubmit = addTask,
                )),

                Toolbar(Props(ToolbarProps,
                    .filter = filter,
                    .onSetFilter = setFilter,
                    .onClearDone = clearDone,
                    .hasDone = stats.done > 0,
                )),

                TaskList(Props(TaskListProps,
                    .store = store,
                    .filter = filter,
                    .hoveredTaskId = hoveredTaskId,
                    .onToggleTask = toggleTask,
                    .onRemoveTask = removeTask,
                    .onHoverTask = hoverTask,
                    .onUnhoverTask = unhoverTask,
                    .onContextTask = Prevent(contextTask),
                )),

                Show(stats.highPriorityOpen > 0,
                    p(id("high-priority-warning"), class(U(TextSm)),
                        mark(class(U(twBg(TwAmber, 100), twTextColor(TwAmber, 800),
                                     twPx(2), twPy(1), twRounded(TwRounded))),
                            text("%d high-priority task(s) still open.",
                                stats.highPriorityOpen)))),

                RemoteTodos(),

                DebugPanel(Props(DebugPanelProps,
                    .renderCount = renderCount(),
                    .lastChangedTaskId = lastChangedTaskId,
                    .hoveredTaskId = hoveredTaskId,
                    .editingDraft = editingDraft,
                    .vpW = vpW,
                    .vpH = vpH,
                    .previousFilter = previousFilter.ok
                        ? (TaskFilter)previousFilter.value
                        : filter,
                    .currentFilter = filter,
                )),

                p(id("store-summary"), class(U(TextXs, FgSlate500)),
                    text("%s", summary))
            ),
        ))
    );
}

app(DashboardApp, {
    .title = "Task Dashboard",
});
