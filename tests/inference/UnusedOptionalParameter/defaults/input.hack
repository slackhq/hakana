function optional(int $required, int $used = 2, string $unused = 'default'): string {
	return (string)($required + $used).$unused;
}
function same_as_default(int $option = 1): int { return $option; }
function unknown_unpack(int $option = 1): int { return $option; }
function escaped(int $option = 1): int { return $option; }
function variadic(int ...$options): vec<int> { return $options; }
function suppressed(
	/* HAKANA_FIXME[UnusedOptionalParameter] Public option. */
	int $option = 1,
): int { return $option; }
async function async_option(bool $option = false): Awaitable<bool> { return $option; }

function production(vec<int> $args): void {
	echo optional(1, 3);
	echo same_as_default(1);
	echo unknown_unpack(...$args);
	$fn = escaped<>;
	echo $fn();
	var_dump(variadic());
	echo suppressed();
	\HH\Asio\join(async_option());
}
