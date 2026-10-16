<<file: __EnableUnstableFeatures('case_types')>>

case type choice_type = int | string;
newtype wrapped_choice = choice_type;

<<Hakana\TypeAssertion('$value', 'T')>>
function verify<reify T>(mixed $value): ?T {
    return $value ?as T;
}

function alias_union(wrapped_choice $value): void {
    verify<string>($value);
    $value ?as string;
}

newtype EncodedId as string = string;
newtype AppId as EncodedId = EncodedId;

function nested_alias_tuple((string, bool) $item, AppId $id): bool {
    return \HH\Lib\C\contains(vec[$item], vec[$id, true]);
}
