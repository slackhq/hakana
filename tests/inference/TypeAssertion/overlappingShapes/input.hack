type left_type = shape('id' => int, ?'extra' => int);
type right_type = shape('id' => int, ?'extra' => string);
type optional_int_type = shape(?'value' => int);
type optional_string_type = shape(?'value' => string);

<<Hakana\TypeAssertion('$value', 'T')>>
function verify<reify T>(mixed $value): ?T {
    return $value ?as T;
}

function shapes(left_type $value, optional_int_type $optional): void {
    verify<right_type>($value);
    $value ?as right_type;
    verify<optional_string_type>($optional);
    $optional ?as optional_string_type;
}

function optional_extra_fields(shape(?'extra' => int) $value): void {
    verify<dict<string, string>>($value);
    $value ?as dict<string, string>;
}

function open_shapes(shape('id' => int, ...) $value): void {
    verify<shape('id' => int, 'name' => string)>($value);
    $value ?as shape('id' => int, 'name' => string);
}

function conditional_fields(?string $action): void {
    $result = shape();
    if ($action is nonnull) {
        $result['action'] = $action;
    }
    if ($result !== shape()) {
        echo 'present';
    }
}

function dynamic_keys(string $key, bool $value): void {
    verify<shape(?'enabled' => bool)>(dict[$key => $value]);
    dict[$key => $value] ?as shape(?'enabled' => bool);
}

function open_optional_fields(shape('type' => string, ...) $value): void {
    verify<shape('type' => string, ?'summary' => vec<string>, ...)>($value);
    $value ?as shape('type' => string, ?'summary' => vec<string>, ...);
}
