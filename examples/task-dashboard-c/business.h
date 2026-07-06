/* business.h — plain C task-store logic. No UI dependency. */
#pragma once

#include <stdbool.h>

typedef signed int bi32;

#define MAX_TASKS 128
#define TASK_TITLE_MAX 96

typedef enum {
    FilterAll = 0,
    FilterOpen = 1,
    FilterDone = 2,
} TaskFilter;

typedef struct {
    bi32 id;
    char title[TASK_TITLE_MAX];
    bool done;
    bi32 priority; /* 1 low, 2 normal, 3 high */
} Task;

typedef struct {
    Task items[MAX_TASKS];
    bi32 count;
    bi32 nextId;
} TaskStore;

typedef struct {
    bi32 total;
    bi32 open;
    bi32 done;
    bi32 highPriorityOpen;
} TaskStats;

void TaskStore_Init(TaskStore *store);
bool TaskStore_Add(TaskStore *store, const char *title, bi32 priority);
bool TaskStore_Toggle(TaskStore *store, bi32 id);
bool TaskStore_Remove(TaskStore *store, bi32 id);
void TaskStore_ClearDone(TaskStore *store);

TaskStats TaskStore_Stats(const TaskStore *store);
bool Task_MatchesFilter(const Task *task, TaskFilter filter);
bi32 TaskStore_CountMatching(const TaskStore *store, TaskFilter filter);

const char *Filter_Label(TaskFilter filter);
TaskFilter Filter_Next(TaskFilter filter);
