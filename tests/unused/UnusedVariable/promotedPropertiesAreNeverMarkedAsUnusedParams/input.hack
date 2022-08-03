class Container {
    private function __construct(
        public float $value
    ) {}

    public static function fromValue(float $value): Container {
        return new self($value);
    }
}