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
        If(props.domId != 0, id(props.domId)),
        type("button"),
        disabled(props.isDisabled),
        onClick(props.onPress),
        class(U(RoundedXl, Px(4), Py(2), TextSm, Cursor("pointer"))),
        classIf(strEq(props.tone, "primary"), BgSlate900, FgWhite, Hover(BgSlate700)),
        classIf(strEq(props.tone, "danger"), BgRed600, FgWhite, Hover(BgRed700)),
        classIf(strEq(props.tone, "plain"), BgWhite, FgSlate900, BorderSlate300),
        classIf(props.isDisabled, Opacity60),
        classIf(theme.compact, Px(3), Py(1)),
        props.label
    );
}

typedef struct {
    const char *label;
    i32 amount; /* numbers as i32 props: the child formats via Text() */
    i32 warn;
} StatPillProps;

component(StatPill, props, StatPillProps) {
    return div(
        class(U(RoundedXl, BorderSlate200, BgWhite, Px(4), Py(3), Flex, FlexCol, Gap(1))),
        span(class(U(TextXs, FgSlate500)), props.label),
        strong(
            class(U(TextLg, FontSemibold)),
            classIf(props.warn, FgAmber500),
            classIf(!props.warn, FgSlate900),
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
        class(U(Flex, FlexCol, Gap(2))),
        p(class(U(TextXs, FgSlate500)), props.appName),
        h1(class(U(Text4xl, FontBold, FgSlate900)), props.title),
        p(class(U(TextSm, FgSlate600)), props.subtitle)
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
        class(U(Block, BgSlate100, FgSlate900, Px(6), Py(8))),
        div(
            class(U(MaxW(200), MxAuto, Flex, FlexCol, Gap(5))),
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
        class(U(RoundedXl, BorderSlate200, BgWhite, Pad(5), Flex, FlexCol, Gap(4))),
        props.rootExtra,
        If(props.title && props.title[0],
            h2(class(U(TextLg, FontSemibold, FgSlate900)), props.title)),
        props.children
    );
}

/* ------------------------------------------------------------- stats grid */

typedef struct {
    TaskStats stats;
} StatsGridProps;

component(StatsGrid, props, StatsGridProps) {
    return div(
        class(U(Grid, GridCols4, Gap(3))),
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
                class(U(Flex, Gap(2))),
                input(
                    keyed("draft-input"), /* host node survives re-renders:
                                             focus + caret genuinely preserved */
                    id("task-title"),
                    type("text"),
                    value(props.draftTitle),
                    onInput(props.onTitleInput),
                    placeholder("What needs doing?"),
                    class(U(WFull, RoundedXl, BorderSlate300, BgWhite, Px(4), Py(3), TextSm))
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
                span(class(U(TextSm, FgSlate600)),
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
        class(U(Flex, Gap(2), ItemsCenter, JustifyBetween)),
        div(
            class(U(Flex, Gap(2), ItemsCenter)),
            span(class(U(TextSm, FgSlate600)),
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
        class(U(Flex, ItemsCenter, JustifyBetween, Gap(3), RoundedXl,
                BorderSlate200, BgWhite, Px(4), Py(3))),
        classIf(task->done, Opacity60),
        div(
            class(U(Flex, FlexCol, Gap(1))),
            span(
                class(U(TextSm, FontSemibold, FgSlate900)),
                classIf(task->done, LineThrough),
                task->title
            ),
            span(class(U(TextXs, FgSlate500)),
                text("Priority %d - %s theme", task->priority, theme.accentName))
        ),
        div(
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
    Handler onToggleTask;   /* behavior */
    Handler onRemoveTask;
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
                div(
                    class(U(Flex, FlexCol, Gap(2))),
                    mapKeyedIf(task, props.store->items, props.store->count,
                        task->id,
                        Task_MatchesFilter(task, props.filter),
                        TaskRow(Props(TaskRowProps,
                            .task = task,
                            .onToggle = bindI32(props.onToggleTask, task->id),
                            .onRemove = bindI32(props.onRemoveTask, task->id),
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
                p(class(U(TextSm, FgSlate500)), "Loading remote todos..."),
                IfElse(q.err,
                    p(class(U(TextSm, FgAmber500)),
                        text("Fetch failed (HTTP %d): %s", (i32)q.httpStatus, q.data)),
                    div(
                        class(U(Flex, FlexCol, Gap(1))),
                        map(t, remoteTitles, remoteTitleCount,
                            p(id("remote-row"), class(U(TextSm, FgSlate600)),
                                text("- %s", *t)))
                    ))),
            div(
                class(U(Flex, Gap(2))),
                AppButton(Props(AppButtonProps,
                    .label = "Refetch",
                    .tone = "plain",
                    .domId = "refetch-remote",
                    .onPress = refetchRemote,
                    .isDisabled = q.loading,
                ))
            )
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
            p(class(U(TextXs, FgSlate500)),
                text("Render count: %d", props.renderCount)),
            p(class(U(TextXs, FgSlate500)),
                text("Last changed task id: %d", props.lastChangedTaskId)),
            p(class(U(TextXs, FgSlate500)),
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

component(DashboardApp, props, DashboardAppProps) {
    stateStruct(TaskStore, store, TaskStore_Init);
    stateStr(draftTitle, "");
    stateI32(draftPriority, 2);
    stateEnum(TaskFilter, filter, FilterAll);
    stateI32(lastChangedTaskId, 0);
    previousI32(previousFilter, filter);

    /* Derived business data (recomputed each render; events ran already). */
    TaskStats stats = TaskStore_Stats(store);

    /* Effects: recorded during the commit pass, executed after the tree is
     * applied. Mount-once, and re-run on filter change (cleanup gets the
     * previous filter's context). */
    useEffect("dash-mount", onDashboardMount, 0, deps0());
    useEffectCtx("filter-sync", onFilterApplied, onFilterLeave,
        ((FilterFxCtx){ .filter = filter }), depsI32((i32)filter));

    eventInput(updateDraftTitle, e) {
        logf("[dashboard] draft title -> \"%s\"", e.value);
        set(draftTitle, e.value);
    }

    event(cyclePriority) {
        i32 next = draftPriority >= 3 ? 1 : draftPriority + 1;
        logf("[dashboard] priority %d -> %d", draftPriority, next);
        set(draftPriority, next);
    }

    event(addTask) {
        if (TaskStore_Add(store, draftTitle, draftPriority)) {
            logf("[dashboard] added task #%d \"%s\" (priority %d), %d total",
                store->nextId - 1, draftTitle, draftPriority, store->count);
            set(draftTitle, "");
            set(draftPriority, 2);
        } else {
            logWarnf("[dashboard] add rejected (empty title or store full)");
        }
    }

    event(cycleFilter) {
        logf("[dashboard] filter %s -> %s",
            Filter_Label(filter), Filter_Label(Filter_Next(filter)));
        set(filter, Filter_Next(filter));
    }

    event(clearDone) {
        i32 before = store->count;
        TaskStore_ClearDone(store);
        logf("[dashboard] cleared %d done task(s), %d remain",
            before - store->count, store->count);
    }

    eventI32(toggleTask, taskId) {
        if (TaskStore_Toggle(store, taskId)) {
            logf("[dashboard] toggled task #%d", taskId);
            set(lastChangedTaskId, taskId);
        }
    }

    eventI32(removeTask, taskId) {
        if (TaskStore_Remove(store, taskId)) {
            logf("[dashboard] removed task #%d, %d remain", taskId, store->count);
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
                    p(id("high-priority-warning"), class(U(TextSm, FgAmber500)),
                        text("%d high-priority task(s) still open.", stats.highPriorityOpen))),

                RemoteTodos(),

                DebugPanel(Props(DebugPanelProps,
                    .renderCount = renderCount(),
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
