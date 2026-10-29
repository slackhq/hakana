final class MapSpec<Tk as arraykey, Tv, T as ConstMap<Tk, Tv>> {
    public function __construct(private classname<T> $what) {}

    private function key(mixed $key): Tk {
        invariant_violation('stub');
    }

    private function value(mixed $value): Tv {
        invariant_violation('stub');
    }

    public function convert(mixed $value): T {
        invariant($value is ConstMap<_, _>, 'expected a map');
        $tuples = $value->mapWithKey(($key, $value) ==> tuple($this->key($key), $this->value($value)));
        $value = new Map(\HH\Lib\Dict\from_entries($tuples));
        if ($this->what === Map::class) {
            /* HH_FIXME[4110] */
            return $value;
        }
        /* HH_FIXME[4110] */
        return $value->immutable();
    }
}

function makeMap<Tk as arraykey, Tv>(vec<(Tk, Tv)> $entries): Map<Tk, Tv> {
    return new Map(\HH\Lib\Dict\from_entries($entries));
}
