abstract class BaseType {
	abstract public function unwrapType(): NamedType;
}

abstract class NamedType extends BaseType {
	<<__Override>>
	final public function unwrapType(): this {
		return $this;
	}
}

interface INonNullableType {
	require extends BaseType;
}

interface IInputType {
	public function unwrapType(): INamedInputType;
}

interface IInputTypeFor<THackType> extends IInputType {}

interface INonNullableInputTypeFor<THackType as nonnull>
	extends INonNullableType, IInputTypeFor<THackType> {}

interface INamedInputType extends INonNullableInputTypeFor<this::THackType> {
	require extends NamedType;
	abstract const type THackType as nonnull;
}

final class NullableInputType<TInner as nonnull> extends BaseType {
	public function __construct(private INonNullableInputTypeFor<TInner> $inner_type) {}

	<<__Override>>
	final public function unwrapType(): INamedInputType {
		return $this->inner_type->unwrapType();
	}
}
