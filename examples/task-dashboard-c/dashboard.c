/* dashboard.c — larger component-model demo for gwbc.h.
 *
 * Demonstrates: nested components, typed props, prop drilling, context,
 * struct/enum/str state, previous state, payload-bound handlers (withI32),
 * imported plain-C business logic, keyed+filtered lists, conditional
 * rendering, reusable layout components with children-as-props, disabled
 * buttons, and a debug panel.
 *
 * Build:
 *   clang --target=wasm32-unknown-unknown -O2 -nostdlib -fno-builtin
 *         -Wl,--no-entry -Wl,--export-memory -I../../sdk-c
 *         -o dashboard-c.wasm dashboard.c business.c
 */
#include "gwbc.h"
#include "business.h"

/* ---------------------------------------------------------------- context */

typedef struct {
    const char *appName;
    const char *accentName;
    i32 compact;
} ThemeValue;

context(ThemeContext, ThemeValue);

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
        PropsOf(
            If(props.domId != 0, id(props.domId)),
            type("button"),
            disabled(props.isDisabled),
            onClick(props.onPress),
            class(U(RoundedXl, Px(4), Py(2), TextSm, Cursor("pointer"))),
            classIf(strEq(props.tone, "primary"), BgSlate900, FgWhite, Hover(BgSlate700)),
            classIf(strEq(props.tone, "danger"), BgRed600, FgWhite, Hover(BgRed700)),
            classIf(strEq(props.tone, "plain"), BgWhite, FgSlate900, BorderSlate300),
            classIf(props.isDisabled, Opacity60),
            classIf(theme.compact, Px(3), Py(1))
        ),
        props.label
    );
}

typedef struct {
    const char *label;
    i32 amount;   /* numbers as i32 props: the child formats via text() */
    i32 warn;
} StatPillProps;

component(StatPill, props, StatPillProps) {
    return div(
        PropsOf(class(U(RoundedXl, BorderSlate200, BgWhite, Px(4), Py(3), Flex, FlexCol, Gap(1)))),
        span(PropsOf(class(U(TextXs, FgSlate500))), props.label),
        strong(
            PropsOf(
                class(U(TextLg, FontSemibold)),
                classIf(props.warn, FgAmber500),
                classIf(!props.warn, FgSlate900)
            ),
            Text(props.amount)
        )
    );
}

/* ---------------------------------------------------------- layout pieces */

typedef struct {
    const char *title;
    const char *subtitle;
    const char *appName;
} HeaderProps;

component(Header, props, HeaderProps) {
    return header(
        PropsOf(class(U(Flex, FlexCol, Gap(2)))),
        p(PropsOf(class(U(TextXs, FgSlate500))), props.appName),
        h1(PropsOf(class(U(Text4xl, FontBold, FgSlate900))), props.title),
        p(PropsOf(class(U(TextSm, FgSlate600))), props.subtitle)
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
        PropsOf(class(U(Block, BgSlate100, FgSlate900, Px(6), Py(8)))),
        div(
            PropsOf(class(U(MaxW(200), MxAuto, Flex, FlexCol, Gap(5)))),
            Header(Props(HeaderProps,
                .title = props.title,
                .subtitle = props.subtitle,
                .appName = theme.appName,
            )),
            props.children
        )
    );
}

typedef struct {
    const char *title;
    Children children;
    Node rootExtra; /* additive prop bundle: propGroup(...) — zero = none */
} CardProps;

component(Card, props, CardProps) {
    return section(
        PropsOf(
            class(U(RoundedXl, BorderSlate200, BgWhite, Pad(5), Flex, FlexCol, Gap(4))),
            props.rootExtra
        ),
        If(props.title && props.title[0],
            h2(PropsOf(class(U(TextLg, FontSemibold, FgSlate900))), props.title)),
        props.children
    );
}

/* ------------------------------------------------------------- stats grid */

typedef struct {
    TaskStats stats;
} StatsGridProps;

component(StatsGrid, props, StatsGridProps) {
    return div(
        PropsOf(class(U(Grid, GridCols4, Gap(3)))),
        StatPill(Props(StatPillProps, .label = "Total", .amount = props.stats.total)),
        StatPill(Props(StatPillProps, .label = "Open", .amount = props.stats.open)),
        StatPill(Props(StatPillProps, .label = "Done", .amount = props.stats.done)),
        StatPill(Props(StatPillProps,
            .label = "High priority",
            .amount = props.stats.highPriorityOpen,
            .warn = props.stats.highPriorityOpen > 0,
        ))
    );
}

/* ----------------------------------------------------------- task composer */

typedef struct {
    const char *draftTitle;
    i32 priority;
    Handler onTitleInput;
    Handler onPriorityCycle;
    Handler onSubmit;
} TaskComposerProps;

component(TaskComposer, props, TaskComposerProps) {
    return Card(Props(CardProps,
        .title = "Add task",
        .children = Children(
            div(
                PropsOf(class(U(Flex, Gap(2)))),
                input(
                    PropsOf(
                        id("task-title"),
                        type("text"),
                        value(props.draftTitle),
                        onInput(props.onTitleInput),
                        placeholder("What needs doing?"),
                        class(U(WFull, RoundedXl, BorderSlate300, BgWhite, Px(4), Py(3), TextSm))
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
                PropsOf(class(U(Flex, Gap(2), ItemsCenter))),
                span(PropsOf(class(U(TextSm, FgSlate600))),
                    text("Priority: %d", props.priority)),
                AppButton(Props(AppButtonProps,
                    .label = "Cycle priority",
                    .tone = "plain",
                    .domId = "cycle-priority",
                    .onPress = props.onPriorityCycle,
                ))
            )
        ),
    ));
}

/* ---------------------------------------------------------------- toolbar */

typedef struct {
    TaskFilter filter;
    Handler onCycleFilter;
    Handler onClearDone;
    i32 hasDone;
} ToolbarProps;

component(Toolbar, props, ToolbarProps) {
    return div(
        PropsOf(class(U(Flex, Gap(2), ItemsCenter, JustifyBetween))),
        div(
            PropsOf(class(U(Flex, Gap(2), ItemsCenter))),
            span(PropsOf(class(U(TextSm, FgSlate600))),
                text("Filter: %s", Filter_Label(props.filter))),
            AppButton(Props(AppButtonProps,
                .label = "Next filter",
                .tone = "plain",
                .domId = "cycle-filter",
                .onPress = props.onCycleFilter,
            ))
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
    Handler onToggle;
    Handler onRemove;
} TaskRowProps;

component(TaskRow, props, TaskRowProps) {
    ThemeValue theme = useContext(ThemeContext);
    const Task *task = props.task;

    return div(
        PropsOf(
            class(U(Flex, ItemsCenter, JustifyBetween, Gap(3), RoundedXl,
                    BorderSlate200, BgWhite, Px(4), Py(3))),
            classIf(task->done, Opacity60)
        ),
        div(
            PropsOf(class(U(Flex, FlexCol, Gap(1)))),
            span(
                PropsOf(
                    class(U(TextSm, FontSemibold, FgSlate900)),
                    classIf(task->done, LineThrough)
                ),
                task->title
            ),
            span(PropsOf(class(U(TextXs, FgSlate500))),
                text("Priority %d - %s theme", task->priority, theme.accentName))
        ),
        div(
            PropsOf(class(U(Flex, Gap(2)))),
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
    const TaskStore *store;
    TaskFilter filter;
    Handler onToggleTask;
    Handler onRemoveTask;
} TaskListProps;

component(TaskList, props, TaskListProps) {
    return Card(Props(CardProps,
        .title = "Tasks",
        .children = Children(
            IfElse(props.store->count == 0,
                p(PropsOf(class(U(TextSm, FgSlate500))), "No tasks yet."),
                div(
                    PropsOf(class(U(Flex, FlexCol, Gap(2)))),
                    mapKeyed(task, props.store->items, props.store->count, task->id,
                        If(Task_MatchesFilter(task, props.filter),
                            TaskRow(Props(TaskRowProps,
                                .task = task,
                                .onToggle = withI32(props.onToggleTask, task->id),
                                .onRemove = withI32(props.onRemoveTask, task->id),
                            ))))
                ))
        ),
    ));
}

/* ------------------------------------------------------------- debug panel */

typedef struct {
    i32 renderCount;
    i32 lastChangedTaskId;
    TaskFilter previousFilter;
    TaskFilter currentFilter;
} DebugPanelProps;

component(DebugPanel, props, DebugPanelProps) {
    return Card(Props(CardProps,
        .title = "Debug",
        .rootExtra = propGroup(BgSlate50),
        .children = Children(
            p(PropsOf(class(U(TextXs, FgSlate500))),
                text("Render count: %d", props.renderCount)),
            p(PropsOf(class(U(TextXs, FgSlate500))),
                text("Last changed task id: %d", props.lastChangedTaskId)),
            p(PropsOf(class(U(TextXs, FgSlate500))),
                text("Filter changed from %s to %s",
                    Filter_Label(props.previousFilter),
                    Filter_Label(props.currentFilter)))
        ),
    ));
}

/* ------------------------------------------------------------------- app */

typedef struct {
    const char *title;
} DashboardAppProps;

/* Plain-C escape hatch for demo purposes: counts every render pass. */
static i32 gRenderPasses;

component(DashboardApp, props, DashboardAppProps) {
    stateStruct(TaskStore, store, TaskStore_Init);
    stateStr(draftTitle, "");
    stateI32(draftPriority, 2);
    stateEnum(TaskFilter, filter, FilterAll);
    stateI32(lastChangedTaskId, 0);
    previousI32(previousFilter, filter);

    gRenderPasses++;

    /* Derived business data (recomputed each render; events ran already). */
    TaskStats stats = TaskStore_Stats(store);

    eventInput(updateDraftTitle, e) {
        set(draftTitle, e.value);
    }

    event(cyclePriority) {
        set(draftPriority, draftPriority >= 3 ? 1 : draftPriority + 1);
    }

    event(addTask) {
        if (TaskStore_Add(store, draftTitle, draftPriority)) {
            set(draftTitle, "");
            set(draftPriority, 2);
        }
    }

    event(cycleFilter) {
        set(filter, Filter_Next(filter));
    }

    event(clearDone) {
        TaskStore_ClearDone(store);
    }

    eventI32(toggleTask, taskId) {
        if (TaskStore_Toggle(store, taskId)) {
            set(lastChangedTaskId, taskId);
        }
    }

    eventI32(removeTask, taskId) {
        if (TaskStore_Remove(store, taskId)) {
            set(lastChangedTaskId, taskId);
        }
    }

    ThemeValue theme = {
        .appName = "GoWebComponents C",
        .accentName = "slate",
        .compact = stats.total > 8,
    };

    return provider(ThemeContext, theme,
        Page(Props(PageProps,
            .title = props.title,
            .subtitle = "State, context, prop drilling, payload handlers, "
                        "keyed lists, and imported business logic - in C.",
            .children = Children(
                StatsGrid(Props(StatsGridProps, .stats = stats)),

                TaskComposer(Props(TaskComposerProps,
                    .draftTitle = draftTitle,
                    .priority = draftPriority,
                    .onTitleInput = updateDraftTitle,
                    .onPriorityCycle = cyclePriority,
                    .onSubmit = addTask,
                )),

                Toolbar(Props(ToolbarProps,
                    .filter = filter,
                    .onCycleFilter = cycleFilter,
                    .onClearDone = clearDone,
                    .hasDone = stats.done > 0,
                )),

                TaskList(Props(TaskListProps,
                    .store = store,
                    .filter = filter,
                    .onToggleTask = toggleTask,
                    .onRemoveTask = removeTask,
                )),

                Show(stats.highPriorityOpen > 0,
                    p(PropsOf(id("high-priority-warning"), class(U(TextSm, FgAmber500))),
                        text("%d high-priority task(s) still open.", stats.highPriorityOpen))),

                DebugPanel(Props(DebugPanelProps,
                    .renderCount = gRenderPasses,
                    .lastChangedTaskId = lastChangedTaskId,
                    .previousFilter = previousFilter.ok
                        ? (TaskFilter)previousFilter.value
                        : filter,
                    .currentFilter = filter,
                ))
            ),
        ))
    );
}

app(DashboardApp, {
    .title = "Task Dashboard",
});
