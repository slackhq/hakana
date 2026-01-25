abstract class Builder<reify TKeys> {
	public function between<T as ?num, TItem as num>(T $column, TItem $min, TItem $max): this where TItem as T {
		return $this;
	}
}

final class ConcreteBuilder extends Builder<int> {}

function test(): void {
	$builder = new ConcreteBuilder();
	$builder->between(1, 2, 3);
}
