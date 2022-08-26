interface IOutputTypeFor<THackType, TResolved> {}

trait TOutputType<THackType, TResolved> implements IOutputTypeFor<THackType, TResolved> {}

final class NullableOutputType<TInner as nonnull, TResolved> {
    use TOutputType<?TInner, ?TResolved>;

    public function __construct() {}
}

final class FieldDefinition<TRet, TResolved> {
    public function __construct(
        private IOutputTypeFor<TRet, TResolved> $type,
    ) {}
}

final class Stringer implements IOutputTypeFor<string, dict<string, mixed>> {
	use TOutputType<string, dict<string, mixed>>;

	final public static function nullableOutput(): NullableOutputType<string, dict<string, mixed>> {
    	throw new \Exception('bad');
    }
}

function foo(): void {
	new FieldDefinition(
	  Stringer::nullableOutput(),
	);
}