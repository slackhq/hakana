interface Selectable<TKey, TValue> {}

class Repository<T> implements Selectable<int,T> {}

interface SomeEntity {}

class SomeRepository extends Repository<SomeEntity> {}