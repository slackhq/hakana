<<file:__EnableUnstableFeatures('case_types')>>

function ordinary(int $old = 1, int $recent = 2): int {
	return $old + $recent;
}

interface Contract {
	public function value(int $old = 1): int;
}
final class Implementation implements Contract {
	public function value(int $old = 1): int { return $old; }
}

function run(Contract $contract): void {
	echo ordinary();
	echo $contract->value();
}
