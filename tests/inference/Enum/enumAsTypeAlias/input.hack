type query_string = string;

enum OrderBy: query_string as query_string {
    NAME = 'name';
    DATE = 'date';
}

function check<T as arraykey>(mixed $_input, HH\enumname<T> $_enum_class): T {
    throw new \Exception('bad');
}

function check_order(mixed $input): OrderBy {
    $typed = check($input, nameof OrderBy);
    return $typed;
}

function as_string(OrderBy $order): string {
    return $order;
}

function as_int(OrderBy $order): int {
    return $order;
}
