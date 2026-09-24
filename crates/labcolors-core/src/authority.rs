//! Процессный контракт семантических полномочий Lab Colors.
//!
//! AUTH-01 намеренно не хранит научный результат и не сериализуется. Три
//! ветви независимы, а право установки выдаётся владельцем ветви только после
//! свежей проверки его доказательства и текущей привязки.

use crate::program_wire::ProgramRendererProvenanceV1;

mod technical_quality;

/// Закрытые ветви семантических полномочий AUTH V1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthorityIdV1 {
    /// Техническая пригодность материализованного цветового поля.
    TechnicalQuality,
    /// Опубликованное соглашение о допустимой cleanliness-семантике.
    CleanConvention,
    /// Опубликованное человеческое доказательство cleanliness с областью переноса.
    HumanCleanEvidence,
}

impl AuthorityIdV1 {
    const fn slot(self) -> usize {
        match self {
            Self::TechnicalQuality => 0,
            Self::CleanConvention => 1,
            Self::HumanCleanEvidence => 2,
        }
    }
}

macro_rules! identity_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            /// Возвращает точную идентичность без выдачи права создавать новую.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }

            const fn from_owner(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }
        }
    };
}

identity_type!(
    AuthorityReleaseIdentityV1,
    "Идентичность выпуска владельца ветви; сама по себе не является доказательством."
);
identity_type!(
    AuthorityApplicabilityIdentityV1,
    "Идентичность точной области применимости; сама по себе не является доказательством."
);
identity_type!(
    AuthorityProvenanceIdentityV1,
    "Идентичность происхождения доказательства; сама по себе не является доказательством."
);

/// Проверенная привязка одной ветви полномочий.
///
/// Поля и конструктор закрыты: внешний вызывающий код может читать полученный
/// описатель, но не превратить сырые байты в полномочие.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AuthorityDescriptorV1 {
    id: AuthorityIdV1,
    release: AuthorityReleaseIdentityV1,
    applicability: AuthorityApplicabilityIdentityV1,
    provenance: AuthorityProvenanceIdentityV1,
}

impl AuthorityDescriptorV1 {
    /// Возвращает ветвь полномочия.
    #[must_use]
    pub const fn id(&self) -> AuthorityIdV1 {
        self.id
    }

    /// Возвращает идентичность выпуска.
    #[must_use]
    pub const fn release_identity(&self) -> AuthorityReleaseIdentityV1 {
        self.release
    }

    /// Возвращает идентичность области применимости.
    #[must_use]
    pub const fn applicability_identity(&self) -> AuthorityApplicabilityIdentityV1 {
        self.applicability
    }

    /// Возвращает идентичность происхождения.
    #[must_use]
    pub const fn provenance_identity(&self) -> AuthorityProvenanceIdentityV1 {
        self.provenance
    }

    const fn from_verified_owner(
        id: AuthorityIdV1,
        release: [u8; 32],
        applicability: [u8; 32],
        provenance: [u8; 32],
    ) -> Self {
        Self {
            id,
            release: AuthorityReleaseIdentityV1::from_owner(release),
            applicability: AuthorityApplicabilityIdentityV1::from_owner(applicability),
            provenance: AuthorityProvenanceIdentityV1::from_owner(provenance),
        }
    }
}

/// Ожидание текущего значения для установки или замены.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityExpectedCurrentV1 {
    /// Ветвь должна быть пустой.
    Vacant,
    /// Ветвь должна содержать ровно это описание.
    Exact(AuthorityDescriptorV1),
}

/// Успешный исход допуска.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityAdmissionOutcomeV1 {
    /// Значение впервые установлено в пустую ветвь.
    Installed,
    /// Уже установлено ровно то же значение; состояние не менялось.
    DuplicateNoop,
    /// Текущее значение заменено после точного сравнения с ожидаемым.
    Replaced,
}

/// Отказы проверки свежего разрешения владельца ветви.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityPermitErrorV1 {
    /// Владелец не предъявил обязательное доказательство ветви.
    MissingProof,
    /// Доказательство относится к другой ветви.
    AuthorityMismatch,
    /// Идентичность выпуска больше не текущая.
    ReleaseMismatch,
    /// Идентичность области применимости больше не текущая.
    ApplicabilityMismatch,
    /// Идентичность происхождения больше не текущая.
    ProvenanceMismatch,
    /// Область требует наблюдение отрисовщика, которого нет.
    RendererObservationRequired,
}

/// Отказы установки в процессное состояние AUTH.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityAdmissionErrorV1 {
    /// Разрешение владельца выдано для другого описания.
    PermitDescriptorMismatch,
    /// Ожидалось установленное значение, но ветвь пуста.
    ExpectedCurrentMissing,
    /// Ветвь занята, но вызывающая сторона ожидала пустое состояние.
    ExpectedCurrentRequired,
    /// Ожидаемое текущее значение относится к другой ветви.
    ExpectedAuthorityMismatch,
    /// Не совпала идентичность текущего выпуска.
    ExpectedReleaseMismatch,
    /// Не совпала идентичность текущей области применимости.
    ExpectedApplicabilityMismatch,
    /// Не совпала идентичность текущего происхождения.
    ExpectedProvenanceMismatch,
}

/// Отказы точного требования установленного полномочия.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityRequireErrorV1 {
    /// В выбранной ветви ничего не установлено.
    Missing,
    /// Не совпал выпуск.
    ReleaseMismatch,
    /// Не совпала область применимости.
    ApplicabilityMismatch,
    /// Не совпало происхождение.
    ProvenanceMismatch,
}

/// Одноразовое разрешение владельца ветви на точное описание.
///
/// Тип специально не реализует `Clone`/`Copy`. Его поля закрыты, а `admit`
/// потребляет значение. Заимствование удерживает текущее состояние владельца
/// неизменяемым до точки записи в `AuthorityStateV1`.
pub struct AuthorityAdmissionPermitV1<'a> {
    owner_current: &'a AuthorityOwnerCurrentV1,
    descriptor: AuthorityDescriptorV1,
}

/// Три независимые процессные ячейки AUTH V1.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AuthorityStateV1 {
    slots: [Option<AuthorityDescriptorV1>; 3],
}

impl AuthorityStateV1 {
    /// Создаёт пустое процессное состояние из трёх ветвей.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [None, None, None],
        }
    }

    /// Читает только выбранную ветвь. Запасного поиска по другим ветвям нет.
    #[must_use]
    pub const fn read(&self, id: AuthorityIdV1) -> Option<AuthorityDescriptorV1> {
        self.slots[id.slot()]
    }

    /// Устанавливает или заменяет одну ветвь после свежего разрешения владельца.
    pub fn admit(
        &mut self,
        next: AuthorityDescriptorV1,
        expected: AuthorityExpectedCurrentV1,
        permit: AuthorityAdmissionPermitV1<'_>,
    ) -> Result<AuthorityAdmissionOutcomeV1, AuthorityAdmissionErrorV1> {
        if permit.descriptor != next || permit.owner_current.descriptor != next {
            return Err(AuthorityAdmissionErrorV1::PermitDescriptorMismatch);
        }

        let slot = next.id.slot();
        let current = self.slots[slot];
        if current == Some(next) {
            return Ok(AuthorityAdmissionOutcomeV1::DuplicateNoop);
        }

        match (current, expected) {
            (None, AuthorityExpectedCurrentV1::Vacant) => {
                self.slots[slot] = Some(next);
                Ok(AuthorityAdmissionOutcomeV1::Installed)
            }
            (None, AuthorityExpectedCurrentV1::Exact(_)) => {
                Err(AuthorityAdmissionErrorV1::ExpectedCurrentMissing)
            }
            (Some(_), AuthorityExpectedCurrentV1::Vacant) => {
                Err(AuthorityAdmissionErrorV1::ExpectedCurrentRequired)
            }
            (Some(current), AuthorityExpectedCurrentV1::Exact(expected)) => {
                ensure_expected_current(current, expected)?;
                self.slots[slot] = Some(next);
                Ok(AuthorityAdmissionOutcomeV1::Replaced)
            }
        }
    }

    /// Требует точную установленную привязку в выбранной ветви.
    pub fn require(&self, expected: AuthorityDescriptorV1) -> Result<(), AuthorityRequireErrorV1> {
        let Some(current) = self.read(expected.id) else {
            return Err(AuthorityRequireErrorV1::Missing);
        };
        if current.release != expected.release {
            return Err(AuthorityRequireErrorV1::ReleaseMismatch);
        }
        if current.applicability != expected.applicability {
            return Err(AuthorityRequireErrorV1::ApplicabilityMismatch);
        }
        if current.provenance != expected.provenance {
            return Err(AuthorityRequireErrorV1::ProvenanceMismatch);
        }
        Ok(())
    }
}

impl Default for AuthorityStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

fn ensure_expected_current(
    current: AuthorityDescriptorV1,
    expected: AuthorityDescriptorV1,
) -> Result<(), AuthorityAdmissionErrorV1> {
    if expected.id != current.id {
        return Err(AuthorityAdmissionErrorV1::ExpectedAuthorityMismatch);
    }
    if expected.release != current.release {
        return Err(AuthorityAdmissionErrorV1::ExpectedReleaseMismatch);
    }
    if expected.applicability != current.applicability {
        return Err(AuthorityAdmissionErrorV1::ExpectedApplicabilityMismatch);
    }
    if expected.provenance != current.provenance {
        return Err(AuthorityAdmissionErrorV1::ExpectedProvenanceMismatch);
    }
    Ok(())
}

// Владелец-side часть остаётся закрытой внутри AUTH. TQ/CC/HCE добавят свои
// конструкторы доказательств отдельными узлами и не получают общий публичный mint.
struct AuthorityOwnerCurrentV1 {
    descriptor: AuthorityDescriptorV1,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "AUTH-01 закрывает выпуск разрешений до конструкторов владельцев TQ/CC/HCE"
    )
)]
impl AuthorityOwnerCurrentV1 {
    const fn new(descriptor: AuthorityDescriptorV1) -> Self {
        Self { descriptor }
    }

    fn advance_verified(
        &mut self,
        descriptor: AuthorityDescriptorV1,
    ) -> Result<(), AuthorityPermitErrorV1> {
        if descriptor.id != self.descriptor.id {
            return Err(AuthorityPermitErrorV1::AuthorityMismatch);
        }
        self.descriptor = descriptor;
        Ok(())
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "квитанция наблюдения отрисовщика принадлежит будущему владельцу TQ"
    )
)]
#[derive(Clone, Copy)]
enum RendererObservationRequirementV1 {
    NotRequired,
    Required,
}

fn issue_permit<'a>(
    owner_current: &'a AuthorityOwnerCurrentV1,
    next: AuthorityDescriptorV1,
    proof_present: bool,
    renderer_requirement: RendererObservationRequirementV1,
    renderer_provenance: ProgramRendererProvenanceV1,
) -> Result<AuthorityAdmissionPermitV1<'a>, AuthorityPermitErrorV1> {
    if !proof_present {
        return Err(AuthorityPermitErrorV1::MissingProof);
    }
    if owner_current.descriptor.id != next.id {
        return Err(AuthorityPermitErrorV1::AuthorityMismatch);
    }
    if owner_current.descriptor.release != next.release {
        return Err(AuthorityPermitErrorV1::ReleaseMismatch);
    }
    if owner_current.descriptor.applicability != next.applicability {
        return Err(AuthorityPermitErrorV1::ApplicabilityMismatch);
    }
    if owner_current.descriptor.provenance != next.provenance {
        return Err(AuthorityPermitErrorV1::ProvenanceMismatch);
    }
    match (renderer_requirement, renderer_provenance) {
        (RendererObservationRequirementV1::NotRequired, _) => {}
        (RendererObservationRequirementV1::Required, ProgramRendererProvenanceV1::Unverified) => {
            return Err(AuthorityPermitErrorV1::RendererObservationRequired);
        }
    }

    Ok(AuthorityAdmissionPermitV1 {
        owner_current,
        descriptor: next,
    })
}

#[cfg(test)]
mod tests;
