final class YieldLambdaMembership {}

// An async lambda containing yield is an async generator, not
// Awaitable<void>
function gen(?int $constraint): AsyncIterator<YieldLambdaMembership> {
	if ($constraint is null) {
		throw new \Exception('x');
	}
	return (
		async () ==> {
			if ($constraint > 5) {
				yield new YieldLambdaMembership();
			}
		}
	)();
}
