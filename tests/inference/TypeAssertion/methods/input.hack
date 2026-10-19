type int_type = int;

abstract class TypeSpec<T> {
    <<Hakana\TypeAssertion('$value', 'T')>>
    public function assertType(mixed $value): T {
        throw new Exception();
    }

    <<Hakana\TypeAssertion('$value', 'U')>>
    public function verifyType<U>(mixed $value, typename<U> $type): ?U {
        throw new Exception();
    }

    <<Hakana\TypeAssertion('$value', 'U')>>
    public static function checkType<reify U>(mixed $value): bool {
        return $value is U;
    }
}

final class IntSpec extends TypeSpec<int> {}

function test(TypeSpec<int> $spec, IntSpec $inherited, string $bad, int $good): void {
    $spec->assertType($bad);
    $spec->assertType($good);
    $spec->verifyType($bad, int_type::class);
    $spec->verifyType($good, int_type::class);
    TypeSpec::checkType<int>($bad);
    TypeSpec::checkType<int>($good);
    $inherited->assertType($bad);
    $inherited->assertType($good);
}
