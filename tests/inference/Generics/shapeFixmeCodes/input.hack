function identity<T>(T $value): T { return $value; }

function missing(): shape('id' => int) {
    /* HH_FIXME[4057] */
    return identity(shape());
}

function optional(shape(?'id' => int) $value): shape('id' => int) {
    /* HH_FIXME[4163] */
    return identity($value);
}

function extra_field(): shape() {
    /* HH_FIXME[4138] */
    return identity(shape('id' => 1));
}
