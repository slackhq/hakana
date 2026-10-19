class ParentClass {
	public function inherited(int $x = 1): int { return $x; }
	public function used(int $x = 1): int { return $x; }
	public function unused_override(int $x = 1): int { return $x; }
	public static async function asyncValue(): Awaitable<int> { return 1; }
}
final class Child extends ParentClass {
	public function used(int $x = 1): int { return $x; }
	public function unused_override(int $x = 1): int { return $x; }
	public function methods(): void {
		$this->privateValue();
	}
	private function privateValue(): int { return 1; }
}
interface Contract {
	public function value(int $x = 1): int;
}
final class Implementation implements Contract {
	public function value(int $x = 1): int { return $x; }
}
async function production(Child $child, ParentClass $parent, Contract $contract): Awaitable<void> {
	$child->inherited();
	$parent->used();
	echo $child->used(2);
	$parent->unused_override();
	await Child::asyncValue();
	echo $contract->value(2);
	make_child()->inherited();
}

function make_child(): Child { return new Child(); }
