final class A<T> {
	public ?B<T> $b = null;
    public function __construct(public T $value) {}
}

final class B<T> {
	public function __construct(public T $value) {}
}
