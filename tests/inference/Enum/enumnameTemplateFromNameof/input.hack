enum Suit: string as string {
    HEARTS = 'h';
    SPADES = 's';
}

function check<T as arraykey>(mixed $_input, HH\enumname<T> $_enum_class): T {
    throw new \Exception('bad');
}

function from_nameof(mixed $input): Suit {
    $typed = check($input, nameof Suit);
    return $typed;
}

function from_class(mixed $input): Suit {
    $typed = check($input, Suit::class);
    return $typed;
}

type my_enumname<T> = classname<HH\BuiltinEnum<T>>;

function check_alias<T as arraykey>(mixed $_input, my_enumname<T> $_enum_class): T {
    throw new \Exception('bad');
}

function from_alias(mixed $input): Suit {
    $typed = check_alias($input, Suit::class);
    return $typed;
}

function wrong_type(mixed $input): int {
    return check($input, Suit::class);
}
