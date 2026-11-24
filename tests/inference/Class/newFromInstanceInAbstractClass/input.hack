<<__ConsistentConstruct>>
abstract class Job {
    public function __construct(protected int $id) {}

    public function next(): this {
        return new $this($this->id + 1);
    }

    public static function make(): Job {
        return new Job(1);
    }
}
