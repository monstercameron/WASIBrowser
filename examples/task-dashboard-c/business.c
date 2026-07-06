#include "business.h"

static void copyTitle(char *dst, const char *src) {
    bi32 i = 0;
    if (!src) {
        dst[0] = 0;
        return;
    }
    while (src[i] && i < TASK_TITLE_MAX - 1) {
        dst[i] = src[i];
        i++;
    }
    dst[i] = 0;
}

void TaskStore_Init(TaskStore *store) {
    store->count = 0;
    store->nextId = 1;

    TaskStore_Add(store, "Wire up nested components", 3);
    TaskStore_Add(store, "Implement context provider", 3);
    TaskStore_Add(store, "Port keyed list helpers to C", 2);
}

bool TaskStore_Add(TaskStore *store, const char *title, bi32 priority) {
    if (!store || store->count >= MAX_TASKS) return false;
    if (!title || title[0] == 0) return false;

    Task *task = &store->items[store->count++];
    task->id = store->nextId++;
    task->done = false;
    task->priority = priority;
    copyTitle(task->title, title);
    return true;
}

bool TaskStore_Toggle(TaskStore *store, bi32 id) {
    if (!store) return false;
    for (bi32 i = 0; i < store->count; i++) {
        if (store->items[i].id == id) {
            store->items[i].done = !store->items[i].done;
            return true;
        }
    }
    return false;
}

bool TaskStore_Remove(TaskStore *store, bi32 id) {
    if (!store) return false;
    for (bi32 i = 0; i < store->count; i++) {
        if (store->items[i].id == id) {
            for (bi32 j = i; j < store->count - 1; j++) {
                store->items[j] = store->items[j + 1];
            }
            store->count--;
            return true;
        }
    }
    return false;
}

void TaskStore_ClearDone(TaskStore *store) {
    if (!store) return;
    bi32 write = 0;
    for (bi32 read = 0; read < store->count; read++) {
        if (!store->items[read].done) {
            store->items[write++] = store->items[read];
        }
    }
    store->count = write;
}

TaskStats TaskStore_Stats(const TaskStore *store) {
    TaskStats stats = {0};
    if (!store) return stats;

    stats.total = store->count;
    for (bi32 i = 0; i < store->count; i++) {
        const Task *task = &store->items[i];
        if (task->done) {
            stats.done++;
        } else {
            stats.open++;
            if (task->priority >= 3) stats.highPriorityOpen++;
        }
    }
    return stats;
}

bool Task_MatchesFilter(const Task *task, TaskFilter filter) {
    if (!task) return false;
    switch (filter) {
    case FilterOpen: return !task->done;
    case FilterDone: return task->done;
    case FilterAll:
    default: return true;
    }
}

const char *Filter_Label(TaskFilter filter) {
    switch (filter) {
    case FilterOpen: return "Open";
    case FilterDone: return "Done";
    case FilterAll:
    default: return "All";
    }
}

TaskFilter Filter_Next(TaskFilter filter) {
    switch (filter) {
    case FilterAll: return FilterOpen;
    case FilterOpen: return FilterDone;
    case FilterDone:
    default: return FilterAll;
    }
}
