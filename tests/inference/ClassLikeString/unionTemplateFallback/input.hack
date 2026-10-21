<<file: __EnableUnstableFeatures('union_intersection_type_hints')>>

final class Item implements Base {}

function nullable_class<T>(?\HH\class_or_classname<T> $name): ?class<T> {
	return $name is nonnull ? \HH\classname_to_class($name) : null;
}

function nullable_string(string $name): int {
	return nullable_class($name);
}

function nullable_mixed(mixed $name): int {
	return nullable_class($name);
}

function nullable_known(?classname<Item> $name): ?class<Item> {
	return nullable_class($name);
}

function null_alternative(): null {
	return nullable_class(null);
}

function class_or_int<T>((\HH\class_or_classname<T> | int) $value): ?class<T> {
	return null;
}

function integer_alternative(int $value): null {
	return class_or_int($value);
}

function literal_integer_alternative(): null {
	return class_or_int(1);
}

function uncovered_string(string $value): int {
	return class_or_int($value);
}

function partly_covered_union((int | bool) $value): int {
	return class_or_int($value);
}

function class_or_scalar<T>(
	(\HH\class_or_classname<T> | int | string) $value,
): ?class<T> {
	return null;
}

function scalar_union_alternative(arraykey $value): null {
	return class_or_scalar($value);
}

function class_or_vec<T>((\HH\class_or_classname<T> | vec<T>) $value): vec<T> {
	return vec[];
}

function other_template_alternative(): vec<int> {
	return class_or_vec(vec[1]);
}

function other_template_preserves_bound(): vec<string> {
	return class_or_vec(vec[1]);
}

interface Base {}
final class Child implements Base {}

function class_or_object<T as Base>((\HH\class_or_classname<T> | T) $value): T {
	throw new Exception();
}

function bounded_template_alternative(): Child {
	return class_or_object(new Child());
}

function bounded_template_preserves_bound(): Item {
	return class_or_object(new Child());
}

function nullable_name<T>(?classname<T> $name): ?classname<T> {
	return $name;
}

function nullable_name_string(string $name): int {
	return nullable_name($name);
}

function known_class(): ?class<Item> {
	return class_or_int(Item::class);
}
