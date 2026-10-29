type Row = shape('id' => int);
type Cursor<T> = shape('id_field' => (function(T): int));

function cursor<T>((function(T): int) $id): Cursor<T> {
    return shape('id_field' => $id);
}

function query<T>((function(Cursor<T>): vec<T>) $fetch, Cursor<T> $cursor): vec<T> {
    return $fetch($cursor);
}

function run(): vec<Row> {
    $cursor = cursor((Row $row) ==> $row['id']);
    return query((Cursor<Row> $_): vec<Row> ==> vec[], $cursor);
}
