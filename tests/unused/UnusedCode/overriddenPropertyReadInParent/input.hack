abstract class Page {
    protected string $body_class = '';
    protected static int $priority = 0;
    protected string $never_read = '';

    public function render(): string {
        return $this->body_class.(string)static::$priority;
    }
}

final class HomePage extends Page {
    protected string $body_class = 'home';
    protected static int $priority = 5;
    protected string $never_read = 'home';
}

abstract class Fetcher {
    public function __construct(protected string $id) {}

    public function getId(): string {
        return $this->id;
    }
}

final class RemoteFetcher extends Fetcher {
    public function __construct(protected string $id) {
        parent::__construct($id);
    }
}

<<__EntryPoint>>
function main(): void {
    echo (new HomePage())->render();
    echo (new RemoteFetcher('a'))->getId();
}
