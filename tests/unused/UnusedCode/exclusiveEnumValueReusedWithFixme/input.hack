enum Priority: int {
    LOW = 1;
    MEDIUM = 2;
    HIGH = 3;
    CRITICAL = 4;
}

abstract class AbstractTask {
    abstract const Priority TASK_PRIORITY;
}

final class EmailTask extends AbstractTask {
    const Priority TASK_PRIORITY = Priority::MEDIUM;
}

final class BackupTask extends AbstractTask {
    /* HAKANA_FIXME[ExclusiveEnumValueReused] */
    const Priority TASK_PRIORITY = Priority::MEDIUM;
}

final class NotificationTask extends AbstractTask {
    const Priority TASK_PRIORITY = Priority::HIGH;
}