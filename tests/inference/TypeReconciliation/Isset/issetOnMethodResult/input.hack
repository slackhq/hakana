function foo(
	Box<shape('name' => string)> $box
): void {
    echo $box->get()['name'] ?? null;
}

final class Box<T> {
  public function __construct(private T $value) {}
  public function get(): T { return $this->value; }
}