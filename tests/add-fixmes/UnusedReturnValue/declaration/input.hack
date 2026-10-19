/**
 * A stable API.
 */
<<__Deprecated('use the new API')>>
function value(): int { return 1; }

/** Documentation for the class. */
final class Example {
	/** Documentation for the method. */
	public async function value(): Awaitable<int> { return 1; }

	/**
	 * Documentation ending on the declaration's line.
	 */ public function inlineValue(): int { return 2; }
}

async function run(Example $example): Awaitable<void> {
	value();
	await $example->value();
	$example->inlineValue();
}
