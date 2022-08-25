interface Maybe<T> {}

class Some<T> implements Maybe<T> {
    public function __construct(private T $value) {}

    public function extract(): T {
        return $this->value;
    }
}

function repository(): Maybe<int> {
    return new Some(5);
}

$maybe = repository();

if ($maybe is Some<_>) {
    echo $maybe->extract();
}