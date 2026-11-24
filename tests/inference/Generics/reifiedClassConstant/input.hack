abstract class Filter {
    abstract public static function getStaticType(): string;
}

final class NameFilter extends Filter {
    <<__Override>>
    public static function getStaticType(): string {
        return 'name';
    }
}

function get_filter_type<<<__Enforceable>> reify T as Filter>(): string {
    $filter_class = T::class;
    return $filter_class::getStaticType();
}

function get_filter_class<<<__Enforceable>> reify T as Filter>(): class<T> {
    return T::class;
}

function get_unknown_class(): mixed {
    return Unknown::class;
}
