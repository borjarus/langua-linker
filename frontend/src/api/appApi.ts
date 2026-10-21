import type {
  AppInfoDto,
  AttributionDto,
  BackupDto,
  BackupInput,
  BulkActionInput,
  BulkPreviewDto,
  BulkPreviewInput,
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
  DeleteNoteInput,
  DeleteNoteResultDto,
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
  NoteContentDto,
  NoteDetailsDto,
  OnboardingStateDto,
  PickFileInput,
  PresetDto,
  PreviewInput,
  RateCardInput,
  RatingPreviewDto,
  ResolveWordLinkInput,
  RestoreInput,
  RestorePreviewDto,
  ReviewResultDto,
  SaveNoteInput,
  SavePresetInput,
  SearchInput,
  SearchWordsInput,
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
  UpdateNoteContentInput,
  UpdateSettingsInput,
  UpdateStudySessionInput,
  AssociationDto,
  WordLinkDto,
  WordMatchDto,
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
  listPresets(): Promise<PresetDto[]>
  /** Creates (also clones) or updates a preset; a change applies to every dependent deck. */
  savePreset(input: SavePresetInput): Promise<PresetDto>

  // study
  startStudy(input: StartStudyInput): Promise<StudySessionDto>
  resumeStudy(sessionId: Id): Promise<StudySessionDto>
  /** Pause, resume or end a session; ending early keeps every committed rating. */
  updateStudySession(input: UpdateStudySessionInput): Promise<StudySessionDto>
  getRatingPreview(input: PreviewInput): Promise<RatingPreviewDto>
  rateCard(input: RateCardInput): Promise<ReviewResultDto>
  undoReview(input: UndoInput): Promise<ReviewResultDto>
  applyCardAction(input: CardActionInput): Promise<CardActionResultDto>
  getSessionSummary(sessionId: Id): Promise<SessionSummaryDto>

  // details / editor
  getNoteDetails(cardId: Id): Promise<NoteDetailsDto>
  getNoteContent(noteId: Id): Promise<NoteContentDto>
  getEditorData(): Promise<EditorDataDto>
  checkDuplicate(input: DuplicateCheckInput): Promise<DuplicateCheckDto>
  saveNote(input: SaveNoteInput): Promise<NoteDetailsDto>
  /** Deletes the note with all its cards and their history. */
  deleteNote(input: DeleteNoteInput): Promise<DeleteNoteResultDto>
  updateNoteContent(input: UpdateNoteContentInput): Promise<NoteDetailsDto>
  setFavoriteAssociation(input: SetFavoriteAssociationInput): Promise<AssociationDto[]>
  resolveWordLink(input: ResolveWordLinkInput): Promise<WordLinkDto>
  /** Word notes matching a selected phrase: exact first, then normalized; empty when nothing matches. */
  searchWords(input: SearchWordsInput): Promise<WordMatchDto[]>
  generateAssociation(input: GenerateAssociationInput): Promise<JobDto>

  // browser
  searchCards(input: SearchInput): Promise<CardPageDto>
  previewBulkAction(input: BulkPreviewInput): Promise<BulkPreviewDto>
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
