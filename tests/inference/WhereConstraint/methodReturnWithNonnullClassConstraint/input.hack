abstract class NonnullConstraintNode {
	public function rewrite(): void {}
}

final class NonnullConstraintListItem<+T as ?NonnullConstraintNode> {
	public function __construct(private T $item) {}

	public function getItem(): T {
		return $this->item;
	}

	public function getItemx(): T where T as nonnull {
		return $this->getItem() as nonnull;
	}
}

function nonnull_constraint_instance_of<T>(
	classname<T> $_type,
	mixed $_value,
): T {
	throw new Exception();
}

function use_nonnull_constrained_return(mixed $value): void {
	$item = nonnull_constraint_instance_of(
		NonnullConstraintListItem::class,
		$value,
	);
	$item->getItemx()->rewrite();
}
