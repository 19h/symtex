use std::{pin::Pin, sync::Arc};
use futures::{Stream, StreamExt};
use tonic::{Request, Response, Status};
use arrow_flight::flight_service_server::{FlightService, FlightServiceServer};
use arrow_flight::{Ticket, FlightData};
use arrow_array::{ArrayRef, LargeBinaryArray};
use arrow_schema::{Schema, Field, DataType};
use arrow::record_batch::RecordBatch;
use crate::state::CanonicalState;
use crate::metrics::Metrics;

pub struct FlightSvc { 
    state: Arc<CanonicalState>,
    metrics: Arc<Metrics>,
}

#[tonic::async_trait]
impl FlightService for FlightSvc {
    type DoGetStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;

    async fn do_get(&self, req: Request<Ticket>) -> Result<Response<Self::DoGetStream>, Status> {
        self.metrics.flight_requests_total.inc();
        
        let ticket = req.into_inner().ticket; // opaque bytes
        let map = self.state.valid_flight_tickets.read();
        let snap = map.get(&ticket).cloned()
            .ok_or_else(|| Status::invalid_argument("invalid/expired ticket"))?;
        drop(map);

        // Serialize roaring bitmap as portable bytes into a single LargeBinary cell
        let mut buf = Vec::new();
        snap.serialize_into(&mut buf)
            .map_err(|_| Status::internal("serialize roaring"))?;

        let arr: ArrayRef = Arc::new(LargeBinaryArray::from_iter_values([buf]));
        let schema = Arc::new(Schema::new(vec![
            Field::new("roaring_portable", DataType::LargeBinary, false)
                .with_metadata([
                    ("content_type".into(), "application/x-roaring".into()),
                    ("version".into(), "1".into())
                ].into())
        ]));
        
        let batch = RecordBatch::try_new(schema, vec![arr])
            .map_err(|e| Status::internal(e.to_string()))?;

        // Convert RecordBatch to FlightData stream
        let schema_fd = arrow_flight::utils::flight_data_from_arrow_schema(batch.schema().as_ref());
        let data_fds = arrow_flight::utils::flight_data_from_arrow_batch(&batch, true);
        
        let stream = futures::stream::iter(vec![Ok(schema_fd)])
            .chain(futures::stream::iter(data_fds).map(Ok));
            
        tracing::debug!(ticket_len = ticket.len(), points = snap.len(), "Served Flight ticket");
        
        Ok(Response::new(Box::pin(stream) as Self::DoGetStream))
    }

    // Implement other required methods with appropriate defaults
    type HandshakeStream = Pin<Box<dyn Stream<Item = Result<arrow_flight::HandshakeResponse, Status>> + Send + 'static>>;
    async fn handshake(&self, _req: Request<tonic::Streaming<arrow_flight::HandshakeRequest>>) -> Result<Response<Self::HandshakeStream>, Status> {
        Err(Status::unimplemented("handshake not implemented"))
    }

    type ListFlightsStream = Pin<Box<dyn Stream<Item = Result<arrow_flight::FlightInfo, Status>> + Send + 'static>>;
    async fn list_flights(&self, _req: Request<arrow_flight::Criteria>) -> Result<Response<Self::ListFlightsStream>, Status> {
        Err(Status::unimplemented("list_flights not implemented"))
    }

    async fn get_flight_info(&self, _req: Request<arrow_flight::FlightDescriptor>) -> Result<Response<arrow_flight::FlightInfo>, Status> {
        Err(Status::unimplemented("get_flight_info not implemented"))
    }

    async fn get_schema(&self, _req: Request<arrow_flight::FlightDescriptor>) -> Result<Response<arrow_flight::SchemaResult>, Status> {
        Err(Status::unimplemented("get_schema not implemented"))
    }

    type DoPutStream = Pin<Box<dyn Stream<Item = Result<arrow_flight::PutResult, Status>> + Send + 'static>>;
    async fn do_put(&self, _req: Request<tonic::Streaming<FlightData>>) -> Result<Response<Self::DoPutStream>, Status> {
        Err(Status::unimplemented("do_put not implemented"))
    }

    type DoExchangeStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;
    async fn do_exchange(&self, _req: Request<tonic::Streaming<FlightData>>) -> Result<Response<Self::DoExchangeStream>, Status> {
        Err(Status::unimplemented("do_exchange not implemented"))
    }

    type DoActionStream = Pin<Box<dyn Stream<Item = Result<arrow_flight::Result, Status>> + Send + 'static>>;
    async fn do_action(&self, _req: Request<arrow_flight::Action>) -> Result<Response<Self::DoActionStream>, Status> {
        Err(Status::unimplemented("do_action not implemented"))
    }

    type ListActionsStream = Pin<Box<dyn Stream<Item = Result<arrow_flight::ActionType, Status>> + Send + 'static>>;
    async fn list_actions(&self, _req: Request<arrow_flight::Empty>) -> Result<Response<Self::ListActionsStream>, Status> {
        Err(Status::unimplemented("list_actions not implemented"))
    }
}

pub fn make_server(state: Arc<CanonicalState>, metrics: Arc<Metrics>) -> FlightServiceServer<FlightSvc> {
    FlightServiceServer::new(FlightSvc { state, metrics })
}
