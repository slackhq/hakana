interface Selectable<TKey, TValue> {}

abstract class Repository<T> implements Selectable<int,T> {}

interface SomeEntity {}

final class SomeRepository extends Repository<SomeEntity> {}