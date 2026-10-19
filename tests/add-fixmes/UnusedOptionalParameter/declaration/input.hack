/** A stable API. */
function value(int $one = 1, int $two = 2): int { return $one + $two; }

function run(): void {
	echo value();
}
