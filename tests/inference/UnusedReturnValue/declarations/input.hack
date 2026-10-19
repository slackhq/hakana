function discarded(int $x = 1): int { return $x; }
function sometimes_used(int $x = 1): int { return $x; }
async function discarded_async(int $x = 1): Awaitable<int> { return $x; }
async function joined(): Awaitable<int> { return 1; }
async function no_value(): Awaitable<void> {}
function callback(int $x = 1): int { return $x; }
function unpacked(int $x = 1): int { return $x; }
function never_called(int $x = 1): int { return $x; }
function only_test_calls(int $x = 1): int { return $x; }
function closure_discard(): int { return 1; }
function closure_used(): int { return 1; }
function global_discard(): int { return 1; }
function global_used(): int { return 1; }
async function piped_join(): Awaitable<int> { return 1; }
function conditional(): int { return 1; }

<<Hakana\TestOnly>>
async function test_helper(): Awaitable<void> {
	echo discarded(2);
	echo await joined();
	echo await discarded_async(2);
}

<<Hakana\MustUse>>
function must_use(): int { return 1; }

/* HAKANA_FIXME[UnusedReturnValue] The result is part of an external contract. */
/* HAKANA_IGNORE[UnusedOptionalParameter] The option is part of that contract. */
function suppressed(int $x = 1): int { return $x; }

async function production(vec<int> $args): Awaitable<void> {
	discarded();
	sometimes_used();
	echo sometimes_used(1);
	await discarded_async();
	\HH\Asio\join(joined());
	await no_value();
	$fn = callback<>;
	$fn();
	echo unpacked(...$args);
	$closure = () ==> {
		closure_discard();
		echo closure_used();
	};
	$closure();
	must_use();
	suppressed();
	piped_join() |> \HH\Asio\join($$);
	true ? conditional() : conditional();
}

global_discard();
echo global_used();
