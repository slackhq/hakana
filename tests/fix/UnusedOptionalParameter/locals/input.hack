type options_t = shape(?'enabled' => bool);

function options(string $name, options_t $opts = shape(), ?int $limit = null): string {
	if ($limit is nonnull) {
		$name .= (string)$limit;
	}
	return $name.(string)Shapes::idx($opts, 'enabled', false);
}

final class Example {
	public async function work(
		string $name,
		/* A configurable prefix. */
		string $prefix = 'a,b',
		bool $enabled = true,
	): Awaitable<string> {
		return $prefix.$name.($enabled ? 'yes' : 'no');
	}
}

function inline(/* HAKANA_FIXME[UnusedParameter] */ int $value = 1): int { return $value; }

function generic<<<__Enforceable>> reify T>(T $value, int $limit = 2): T {
	echo $limit;
	return $value;
}

function heredoc(int $value = 2): string {
	return <<<TEXT
{ $value }, it's text.
TEXT;
}

async function run(Example $example): Awaitable<void> {
	echo options('name');
	echo await $example->work('name');
	echo inline();
	echo heredoc();
	echo generic<int>(1);
}
