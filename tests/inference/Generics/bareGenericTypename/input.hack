<<file: __EnableUnstableFeatures('case_types')>>
final class Success<+T> {}
final class Failure {}
case type Either<+T> = Success<T> | Failure;
type Result<T> = Either<T>;

function assert_type<T>(mixed $value, typename<T> $name): T {
    throw new Exception();
}

function result(mixed $value): Result<int> {
    return assert_type($value, Result::class);
}
