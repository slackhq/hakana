<<file: __EnableUnstableFeatures('union_intersection_type_hints')>>

interface Struct {}

function accept_struct(class<Struct> $class): void {}

function convert_validated_string(string $name): void {
	if (!class_exists($name) || !is_a($name, nameof Struct, true)) {
		return;
	}
	accept_struct(\HH\classname_to_class($name));
}

function convert_classname(classname<Struct> $name): class<Struct> {
	return \HH\classname_to_class($name);
}

function convert_mixed(mixed $name): class<mixed> {
	return \HH\classname_to_class($name);
}

function convert_string(string $name): class<mixed> {
	return \HH\classname_to_class($name);
}

function name_from_mixed(mixed $name): classname<mixed> {
	return \HH\class_to_classname($name);
}

function convert_mixed_backdoor(mixed $name): class<mixed> {
	return \HH\classname_to_class_strict_isolation_backdoor($name);
}

function convert_generic<T as Struct>(\HH\class_or_classname<T> $name): class<T> {
	return \HH\classname_to_class($name);
}

function classname_or_int<T>((classname<T> | int) $value): ?class<T> {
	return null;
}

function integer_alternative(): null {
	return classname_or_int(1);
}
