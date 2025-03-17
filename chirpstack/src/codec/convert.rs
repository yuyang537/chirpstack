/*
 * 模块概述
 * ========
 * 该模块是ChirpStack LoRaWAN网络服务器的数据转换工具，用于在不同数据格式之间进行转换。
 * 在ChirpStack的编解码器系统中，需要在JavaScript运行时、Protocol Buffers和JSON等不同格式间转换数据。
 * 该模块提供了这些转换功能，确保数据在不同系统组件间能够无缝流动。
 *
 * 文件功能
 * ========
 * 该文件实现了在QuickJS JavaScript引擎值、Protocol Buffers结构和JSON格式之间的转换功能。
 * 它提供了双向转换函数，支持复杂的嵌套数据结构，并处理特殊情况如NaN值。
 * 主要功能包括将JavaScript对象转换为Protocol Buffers结构，以及反向转换。
 *
 * 主要组件
 * ========
 * - rquickjs_to_struct(): 将QuickJS值转换为Protocol Buffers结构
 * - _rquickjs_to_struct_val(): 递归处理QuickJS值的内部辅助函数
 * - struct_to_rquickjs(): 将Protocol Buffers结构转换为QuickJS对象
 * - _struct_to_rquickjs(): 递归处理Protocol Buffers值的内部辅助函数
 * - pb_json_to_prost(): 在不同Protocol Buffers实现之间转换
 *
 * 关键流程
 * ========
 * 1. JavaScript到Protocol Buffers转换流程:
 *    - 接收QuickJS JavaScript引擎中的值
 *    - 根据值的类型(布尔、数字、字符串、数组、对象)进行相应转换
 *    - 处理特殊情况，如NaN值
 *    - 递归处理嵌套结构
 *    - 返回转换后的Protocol Buffers结构
 *
 * 2. Protocol Buffers到JavaScript转换流程:
 *    - 接收Protocol Buffers结构
 *    - 根据值的类型进行相应转换
 *    - 递归处理嵌套结构
 *    - 返回转换后的QuickJS对象
 *
 * 注意事项
 * ========
 * - 数据类型转换可能导致精度损失，特别是在处理浮点数时
 * - NaN值在Protocol Buffers中无法表示，需要特殊处理
 * - 复杂的嵌套结构可能导致转换性能下降
 * - JavaScript对象的属性名在转换为Protocol Buffers时会保留
 * - 在处理大量数据时，转换性能可能成为瓶颈
 */

pub fn rquickjs_to_struct(val: &rquickjs::Value) -> pbjson_types::Struct {
    if val.type_of() == rquickjs::Type::Object {
        if let Some(pbjson_types::value::Kind::StructValue(v)) = _rquickjs_to_struct_val(val) {
            return v;
        }
    }

    Default::default()
}

fn _rquickjs_to_struct_val(val: &rquickjs::Value) -> Option<pbjson_types::value::Kind> {
    match val.type_of() {
        rquickjs::Type::Bool => Some(pbjson_types::value::Kind::BoolValue(val.as_bool().unwrap())),
        rquickjs::Type::Int => Some(pbjson_types::value::Kind::NumberValue(
            val.as_int().unwrap().into(),
        )),
        rquickjs::Type::Float => {
            let v = val.as_float().unwrap();
            if v.is_nan() {
                // Avoid Cannot serialize NaN as google.protobuf.Value.number_value error.
                None
            } else {
                Some(pbjson_types::value::Kind::NumberValue(v))
            }
        }
        rquickjs::Type::String => Some(pbjson_types::value::Kind::StringValue(
            val.as_string().unwrap().to_string().unwrap(),
        )),
        rquickjs::Type::Array => Some(pbjson_types::value::Kind::ListValue(
            pbjson_types::ListValue {
                values: val
                    .as_array()
                    .unwrap()
                    .iter::<rquickjs::Value>()
                    .map(|v| pbjson_types::Value {
                        kind: _rquickjs_to_struct_val(&v.unwrap()),
                    })
                    .collect(),
            },
        )),
        rquickjs::Type::Object => Some(pbjson_types::value::Kind::StructValue(
            pbjson_types::Struct {
                fields: val
                    .as_object()
                    .unwrap()
                    .clone()
                    .into_iter()
                    .map(|i| {
                        let (k, v) = i.unwrap();
                        (
                            k.to_string().unwrap(),
                            pbjson_types::Value {
                                kind: _rquickjs_to_struct_val(&v),
                            },
                        )
                    })
                    .collect(),
            },
        )),
        _ => None,
    }
}

pub fn struct_to_rquickjs<'js>(
    ctx: &rquickjs::Ctx<'js>,
    obj: &prost_types::Struct,
) -> rquickjs::Object<'js> {
    let out = rquickjs::Object::new(ctx.clone()).unwrap();

    for (k, v) in &obj.fields {
        out.set(k, _struct_to_rquickjs(ctx, v)).unwrap();
    }

    out
}

fn _struct_to_rquickjs<'js>(
    ctx: &rquickjs::Ctx<'js>,
    val: &prost_types::Value,
) -> rquickjs::Value<'js> {
    match &val.kind {
        None => rquickjs::Value::new_null(ctx.clone()),
        Some(val) => match val {
            prost_types::value::Kind::NullValue(_) => rquickjs::Value::new_null(ctx.clone()),
            prost_types::value::Kind::NumberValue(v) => rquickjs::Value::new_float(ctx.clone(), *v),
            prost_types::value::Kind::StringValue(v) => {
                rquickjs::Value::from_string(rquickjs::String::from_str(ctx.clone(), v).unwrap())
            }
            prost_types::value::Kind::BoolValue(v) => rquickjs::Value::new_bool(ctx.clone(), *v),
            prost_types::value::Kind::StructValue(v) => {
                let out = rquickjs::Object::new(ctx.clone()).unwrap();
                for (k, v) in &v.fields {
                    out.set(k, _struct_to_rquickjs(ctx, v)).unwrap();
                }
                rquickjs::Value::from_object(out)
            }
            prost_types::value::Kind::ListValue(v) => {
                let out = rquickjs::Array::new(ctx.clone()).unwrap();
                for (i, v) in v.values.iter().enumerate() {
                    out.set(i, _struct_to_rquickjs(ctx, v)).unwrap();
                }
                rquickjs::Value::from_array(out)
            }
        },
    }
}

pub fn pb_json_to_prost(obj: &pbjson_types::Struct) -> prost_types::Struct {
    let mut out = prost_types::Struct::default();
    for (k, v) in &obj.fields {
        out.fields.insert(k.to_string(), _pb_json_to_prost(v));
    }

    out
}

fn _pb_json_to_prost(v: &pbjson_types::Value) -> prost_types::Value {
    prost_types::Value {
        kind: v.kind.as_ref().map(|v| match v {
            pbjson_types::value::Kind::NullValue(v) => prost_types::value::Kind::NullValue(*v),
            pbjson_types::value::Kind::NumberValue(v) => prost_types::value::Kind::NumberValue(*v),
            pbjson_types::value::Kind::StringValue(v) => {
                prost_types::value::Kind::StringValue(v.to_string())
            }
            pbjson_types::value::Kind::BoolValue(v) => prost_types::value::Kind::BoolValue(*v),
            pbjson_types::value::Kind::StructValue(v) => {
                prost_types::value::Kind::StructValue(prost_types::Struct {
                    fields: v
                        .fields
                        .iter()
                        .map(|(k, v)| (k.to_string(), _pb_json_to_prost(v)))
                        .collect(),
                })
            }
            pbjson_types::value::Kind::ListValue(v) => {
                prost_types::value::Kind::ListValue(prost_types::ListValue {
                    values: v.values.iter().map(_pb_json_to_prost).collect(),
                })
            }
        }),
    }
}
