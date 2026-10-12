import type {
  AppInfoDto,
  AttributionDto,
  BackupDto,
  BackupInput,
  BulkActionInput,
  BulkResultDto,
  CardActionInput,
  CardActionResultDto,
  CardPageDto,
  CollectionChangedEventDto,
  CommitImportInput,
  CompleteOnboardingInput,
  CreateDeckInput,
  DashboardDto,
  DeckOverviewDto,
  DeleteDeckInput,
  DeleteDeckResultDto,
  DuplicateCheckDto,
  DuplicateCheckInput,
  EditorDataDto,
  FileHandleDto,
  GenerateAssociationInput,
  Id,
  ImportInput,
  ImportPreviewDto,
  JobDto,
  JobEventDto,
  NoteDetailsDto,
  OnboardingStateDto,
  PickFileInput,
  PreviewInput,
  RateCardInput,
  RatingPreviewDto,
  ResolveWordLinkInput,
  RestoreInput,
  RestorePreviewDto,
  ReviewResultDto,
  SaveNoteInput,
  SearchInput,
  SessionSummaryDto,
  SetFavoriteAssociationInput,
  SettingsDto,
  StartStudyInput,
  StatsDto,
  StatsInput,
  StudySessionDto,
  UndoInput,
  Unsubscribe,
  UpdateDeckInput,
  UpdateSettingsInput,
  AssociationDto,
  WordLinkDto,
  ValidateRestoreInput,
} from './types'

/**
 * The only gateway from screens to the backend. Implemented by `MockAppApi` (demo, Phases 2–6)
 * and the Tauri adapter (production). Rejections are always `AppError` carrying an `ErrorDto`.
 */
export interface AppApi {
  // app
  getAppInfo(): Promise<AppInfoDto>
  getAttributions(): Promise<AttributionDto[]>

  // dashboard / decks
  getDashboard(): Promise<DashboardDto>
  getDeck(deckId: Id): Promise<DeckOverviewDto>
  createDeck(input: CreateDeckInput): Promise<DeckOverviewDto>
  updateDeck(input: UpdateDeckInput): Promise<DeckOverviewDto>
  deleteDeck(input: DeleteDeckInput): Promise<DeleteDeckResultDto>

  // study
  startStudy(input: StartStudyInput): Promise<StudySessionDto>
  resumeStudy(sessionId: Id): Promise<StudySessionDto>
  getRatingPreview(input: PreviewInput): Promise<RatingPreviewDto>
  rateCard(input: RateCardInput): Promise<ReviewResultDto>
  undoReview(input: UndoInput): Promise<ReviewResultDto>
  applyCardAction(input: CardActionInput): Promise<CardActionResultDto>
  getSessionSummary(sessionId: Id): Promise<SessionSummaryDto>

  // details / editor
  getNoteDetails(cardId: Id): Promise<NoteDetailsDto>
  getEditorData(): Promise<EditorDataDto>
  checkDuplicate(input: DuplicateCheckInput): Promise<DuplicateCheckDto>
  saveNote(input: SaveNoteInput): Promise<NoteDetailsDto>
  setFavoriteAssociation(input: SetFavoriteAssociationInput): Promise<AssociationDto[]>
  resolveWordLink(input: ResolveWordLinkInput): Promise<WordLinkDto>
  generateAssociation(input: GenerateAssociationInput): Promise<JobDto>

  // browser
  searchCards(input: SearchInput): Promise<CardPageDto>
  applyBulkAction(input: BulkActionInput): Promise<BulkResultDto>

  // statistics / settings / onboarding
  getStats(input: StatsInput): Promise<StatsDto>
  getSettings(): Promise<SettingsDto>
  updateSettings(input: UpdateSettingsInput): Promise<SettingsDto>
  getOnboardingState(): Promise<OnboardingStateDto>
  completeOnboarding(input: CompleteOnboardingInput): Promise<JobDto>

  // transfer / backup
  pickFile(input: PickFileInput): Promise<FileHandleDto | null>
  previewImport(input: ImportInput): Promise<ImportPreviewDto>
  commitImport(input: CommitImportInput): Promise<JobDto>
  listBackups(): Promise<BackupDto[]>
  createBackup(input: BackupInput): Promise<JobDto>
  validateRestore(input: ValidateRestoreInput): Promise<RestorePreviewDto>
  restoreBackup(input: RestoreInput): Promise<JobDto>

  // jobs / events
  getJobStatus(jobId: Id): Promise<JobDto>
  cancelJob(jobId: Id): Promise<JobDto>
  /** Synchronous unsubscribe; async registration is handled inside the adapter. */
  onJobEvent(listener: (event: JobEventDto) => void): Unsubscribe
  onCollectionChanged(listener: (event: CollectionChangedEventDto) => void): Unsubscribe
}

export type AppApiOperation = keyof AppApi
