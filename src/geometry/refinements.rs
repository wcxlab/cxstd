use std::fmt::{self, Debug};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::refineable::{IsEmpty, Refineable};
use super::{Bounds, Corners, Edges, Point, Size};

macro_rules! geometry_refinement {
    (
        $(#[$attribute:meta])*
        $geometry:ident => $refinement:ident;
        field attribute: $field_attribute:meta;
        $($field:ident: $ty:ty),+ $(,)?
    ) => {
        #[derive(Clone, Default)]
        $(#[$attribute])*
        pub struct $refinement<T: Clone + Debug + Default + PartialEq> {
            $(#[$field_attribute] pub $field: Option<$ty>),+
        }

        impl<T: Clone + Debug + Default + PartialEq> Debug for $refinement<T> {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut debug = formatter.debug_struct(stringify!($refinement));
                $(
                    if self.$field.is_some() {
                        debug.field(stringify!($field), &self.$field);
                    }
                )+
                if $(self.$field.is_some())&&+ {
                    debug.finish()
                } else {
                    debug.finish_non_exhaustive()
                }
            }
        }

        impl<T: Clone + Debug + Default + PartialEq> $refinement<T> {
            pub fn is_some(&self) -> bool {
                $(self.$field.is_some())||+
            }
        }

        impl<T: Clone + Debug + Default + PartialEq> IsEmpty for $refinement<T> {
            fn is_empty(&self) -> bool {
                !self.is_some()
            }
        }

        impl<T: Clone + Debug + Default + PartialEq> From<$refinement<T>> for $geometry<T> {
            fn from(value: $refinement<T>) -> Self {
                Self { $($field: value.$field.unwrap_or_default()),+ }
            }
        }

        impl<T: Clone + Debug + Default + PartialEq> Refineable for $geometry<T> {
            type Refinement = $refinement<T>;

            fn refine(&mut self, refinement: &Self::Refinement) {
                $(if let Some(value) = &refinement.$field { self.$field = value.clone(); })+
            }

            fn refined(mut self, refinement: Self::Refinement) -> Self {
                $(if let Some(value) = refinement.$field { self.$field = value; })+
                self
            }

            fn is_superset_of(&self, refinement: &Self::Refinement) -> bool {
                $(refinement.$field.as_ref().is_none_or(|value| value == &self.$field))&&+
            }

            fn subtract(&self, refinement: &Self::Refinement) -> Self::Refinement {
                $refinement {
                    $($field: (refinement.$field.as_ref() != Some(&self.$field))
                        .then(|| self.$field.clone())),+
                }
            }
        }

        impl<T: Clone + Debug + Default + PartialEq> Refineable for $refinement<T> {
            type Refinement = Self;

            fn refine(&mut self, refinement: &Self) {
                $(if refinement.$field.is_some() { self.$field.clone_from(&refinement.$field); })+
            }

            fn refined(self, refinement: Self) -> Self {
                Self { $($field: refinement.$field.or(self.$field)),+ }
            }

            fn is_superset_of(&self, refinement: &Self) -> bool {
                $(refinement.$field.is_none() || refinement.$field == self.$field)&&+
            }

            fn subtract(&self, refinement: &Self) -> Self {
                Self {
                    $($field: (self.$field != refinement.$field)
                        .then(|| self.$field.clone()).flatten()),+
                }
            }
        }
    };
}

geometry_refinement!(
    #[derive(PartialEq, Serialize, Deserialize, JsonSchema)]
    Point => PointRefinement;
    field attribute: serde(skip_serializing_if = "Option::is_none");
    x: T, y: T,
);

geometry_refinement!(
    #[derive(PartialEq, Serialize, Deserialize, JsonSchema)]
    Size => SizeRefinement;
    field attribute: serde(skip_serializing_if = "Option::is_none");
    width: T, height: T,
);

geometry_refinement!(
    Bounds => BoundsRefinement;
    field attribute: allow(missing_docs);
    origin: Point<T>, size: Size<T>,
);

geometry_refinement!(
    #[derive(PartialEq, Serialize, Deserialize, JsonSchema)]
    Edges => EdgesRefinement;
    field attribute: serde(skip_serializing_if = "Option::is_none");
    top: T, right: T, bottom: T, left: T,
);

geometry_refinement!(
    #[derive(PartialEq, Serialize, Deserialize, JsonSchema)]
    Corners => CornersRefinement;
    field attribute: serde(skip_serializing_if = "Option::is_none");
    top_left: T, top_right: T, bottom_right: T, bottom_left: T,
);
