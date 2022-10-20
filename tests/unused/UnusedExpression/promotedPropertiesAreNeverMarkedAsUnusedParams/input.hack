class MyContainer {
    private function __construct(
        public float $value
    ) {}

    public static function fromValue(float $value): MyContainer {
        return new self($value);
    }
}