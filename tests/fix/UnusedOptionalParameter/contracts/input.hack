function tested(int $option = 1): int { return $option; }

interface Contract {
	public function value(int $option = 1): int;
}
final class Implementation implements Contract {
	public function value(int $option = 1): int { return $option; }
}
<<Hakana\TestOnly>>
final class MockImplementation implements Contract {
	public function value(int $option = 1): int { return $option; }
}
/** A class comment must stay outside the class. */
final class Promoted {
	public function __construct(public int $option = 1) {}
}

interface TestContract {
	public function value(int $option = 1): int;
}
final class TestImplementation implements TestContract {
	public function value(int $option = 1): int { return $option; }
}

interface NoOptionContract {
	public function value(): int;
}
final class ExtraOption implements NoOptionContract {
	public function value(int $option = 1): int { return $option; }
}

function run(Contract $contract, TestContract $test_contract, NoOptionContract $no_option): void {
	echo tested();
	echo $contract->value();
	echo (new Promoted())->option;
	echo $test_contract->value();
	echo $no_option->value();
}
