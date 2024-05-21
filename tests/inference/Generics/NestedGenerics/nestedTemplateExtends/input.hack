namespace Foo;

interface IBaseViewData {}

abstract class BaseModel<TViewData> {}

abstract class BaseRepository<TViewData as IBaseViewData, TModel as BaseModel<TViewData>> {}

final class StudentViewData implements IBaseViewData {}
final class TeacherViewData implements IBaseViewData {}

final class StudentModel extends BaseModel<StudentViewData> {}
final class TeacherModel extends BaseModel<TeacherViewData> {}

final class StudentRepository extends BaseRepository<StudentViewData, StudentModel> {}
final class TeacherRepository extends BaseRepository<TeacherViewData, TeacherModel>{}