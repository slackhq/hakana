type EmptyMetadata = shape();

interface Parser<T> {
    public function get(): T;
}

final class EmptyParser implements Parser<EmptyMetadata> {
    private EmptyMetadata $metadata;
    public function __construct() { $this->metadata = shape(); }
    <<__Override>>
    public function get(): EmptyMetadata { return $this->metadata; }
}

interface Dispatcher<T> {}
final class Dispatch<T> implements Dispatcher<T> {}

final class Handler<T> {
    public function __construct(public Parser<T> $parser) {}
    public function handle(?Dispatcher<T> $dispatch = null): void {}
}

function equivalent(): void {
    $handler = new Handler(new EmptyParser());
    $handler->handle(new Dispatch<EmptyMetadata>());
}

function same<T>(Parser<T> $parser, Dispatcher<T> $dispatcher): void {}

function equivalent_alias(): void {
    same(new EmptyParser(), new Dispatch<shape()>());
}
