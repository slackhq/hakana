type FileShape = shape('id' => string, ?'count' => int);

function suppressed(vec<FileShape> $values): vec<FileShape> {
    return \HH\Lib\Vec\map($values, $value ==> {
        $value = Shapes::toDict($value);
        foreach ($value as $key => $field) {
            if ($field is string) {
                $value[$key] = 'changed';
            }
        }
        /* HH_FIXME[4110] */
        return $value;
    });
}

function unsuppressed(vec<FileShape> $values): vec<FileShape> {
    return \HH\Lib\Vec\map($values, $value ==> {
        $value = Shapes::toDict($value);
        foreach ($value as $key => $field) {
            if ($field is string) {
                $value[$key] = 'changed';
            }
        }
        return $value;
    });
}

function separate_returns(vec<FileShape> $values, bool $first): vec<FileShape> {
    return \HH\Lib\Vec\map($values, $value ==> {
        $value = Shapes::toDict($value);
        foreach ($value as $key => $field) {
            if ($field is string) {
                $value[$key] = 'changed';
            }
        }
        if ($first) {
            /* HH_FIXME[4110] */
            return $value;
        }
        return $value;
    });
}

function expression_lambda(
    vec<shape('id' => string, 'count' => string)> $values,
): vec<FileShape> {
    return \HH\Lib\Vec\map(
        $values,
        $value ==>
            /* HH_FIXME[4110] */
            $value,
    );
}
