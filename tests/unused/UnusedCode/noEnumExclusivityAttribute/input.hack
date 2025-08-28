<<Hakana\ExclusiveEnumValues>>
enum Status: string {
    ACTIVE = "active";
    INACTIVE = "inactive";
    PENDING = "pending";
}

abstract class AbstractService {
    abstract const Status SERVICE_STATUS;
}

final class ActiveService extends AbstractService {
    const Status SERVICE_STATUS = Status::ACTIVE;
}

final class InactiveService extends AbstractService {
    const Status SERVICE_STATUS = Status::INACTIVE;
}