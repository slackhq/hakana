abstract class Foo {
    private dict<arraykey, mixed> $options;
    private dict<arraykey, mixed> $defaultOptions;

    public function __construct(dict<arraykey, mixed> $options) {
        $this->setOptions($options);
        $this->setDefaultOptions($this->getOptions());
    }

    public function getOptions(): dict<arraykey, mixed> {
        return $this->options;
    }

    public final function setOptions(dict<arraykey, mixed> $options): void {
        $this->options = $options;
    }

    public final function setDefaultOptions(dict<arraykey, mixed> $defaultOptions): void {
        $this->defaultOptions = $defaultOptions;
    }
}