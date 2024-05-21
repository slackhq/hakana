type foo_t = shape(
    'a' => int,
    'b' => string,
);

<<Hakana\Immutable>>
final class A {
    public function __construct(
        public dict<int, int> $arr,
        protected foo_t $foo,
    ) {}

    public function mutate(): void {
        $this->arr[4] = 5;
        $this->foo['a'] = 6;
        $this->foo['b'] = 'a';
    }
}